use std::{
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use chrono::NaiveDate;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{
    error::BankSyncError,
    provider::{
        AspspInfo, BankSyncProvider, ProviderBalance, ProviderBankAccount, ProviderTransaction,
    },
};

const SANDBOX_BASE_URL: &str = "https://bankaccountdata.gocardless.com/api/v2";
const PRODUCTION_BASE_URL: &str = "https://bankaccountdata.gocardless.com/api/v2";

#[derive(Clone)]
pub struct GoCardlessProvider {
    base_url: String,
    refresh_token: String,
    http: Client,
    access_token: Arc<Mutex<Option<CachedAccessToken>>>,
}

#[derive(Clone)]
struct CachedAccessToken {
    value: String,
    expires_at: Instant,
}

impl GoCardlessProvider {
    pub async fn from_env() -> Result<Self, BankSyncError> {
        let secret_id = std::env::var("GOCARDLESS_SECRET_ID")
            .map_err(|_| BankSyncError::Config("GOCARDLESS_SECRET_ID is not set".into()))?;
        let secret_key = std::env::var("GOCARDLESS_SECRET_KEY")
            .map_err(|_| BankSyncError::Config("GOCARDLESS_SECRET_KEY is not set".into()))?;

        let env = std::env::var("GOCARDLESS_ENVIRONMENT").unwrap_or_else(|_| "SANDBOX".into());
        let base_url = if env.eq_ignore_ascii_case("PRODUCTION") {
            PRODUCTION_BASE_URL.to_string()
        } else {
            SANDBOX_BASE_URL.to_string()
        };

        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let refresh_token =
            Self::new_refresh_token(&http, &base_url, &secret_id, &secret_key).await?;

        Ok(Self {
            base_url,
            refresh_token,
            http,
            access_token: Arc::new(Mutex::new(None)),
        })
    }

    pub fn is_configured() -> bool {
        std::env::var("GOCARDLESS_SECRET_ID").is_ok()
            && std::env::var("GOCARDLESS_SECRET_KEY").is_ok()
    }

    async fn new_refresh_token(
        http: &Client,
        base_url: &str,
        secret_id: &str,
        secret_key: &str,
    ) -> Result<String, BankSyncError> {
        let resp = http
            .post(format!("{base_url}/token/new/"))
            .json(&TokenNewRequest {
                secret_id,
                secret_key,
            })
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let resp = Self::check_response(resp).await?;
        let data: TokenNewResponse = resp
            .json()
            .await
            .map_err(|e| BankSyncError::Parse(e.to_string()))?;

        Ok(data.refresh)
    }

    async fn access_token(&self) -> Result<String, BankSyncError> {
        {
            let cache = self.access_token.lock().map_err(|_| {
                BankSyncError::Provider("GoCardless token cache lock poisoned".into())
            })?;
            if let Some(cached) = cache.as_ref() {
                if Instant::now() < cached.expires_at {
                    return Ok(cached.value.clone());
                }
            }
        }

        let resp = self
            .http
            .post(format!("{}/token/refresh/", self.base_url))
            .json(&TokenRefreshRequest {
                refresh: &self.refresh_token,
            })
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let resp = Self::check_response(resp).await?;
        let data: TokenRefreshResponse = resp
            .json()
            .await
            .map_err(|e| BankSyncError::Parse(e.to_string()))?;

        let mut cache = self
            .access_token
            .lock()
            .map_err(|_| BankSyncError::Provider("GoCardless token cache lock poisoned".into()))?;
        let ttl = data.access_expires.saturating_sub(60);
        let token = data.access.clone();
        *cache = Some(CachedAccessToken {
            value: data.access,
            expires_at: Instant::now() + Duration::from_secs(ttl.max(60)),
        });

        Ok(token)
    }

    async fn authorized_get<T: for<'de> Deserialize<'de>>(
        &self,
        url: String,
    ) -> Result<T, BankSyncError> {
        let token = self.access_token().await?;
        let resp = self
            .http
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let resp = Self::check_response(resp).await?;
        resp.json()
            .await
            .map_err(|e| BankSyncError::Parse(e.to_string()))
    }

    async fn authorized_post<B: Serialize + ?Sized, T: for<'de> Deserialize<'de>>(
        &self,
        url: String,
        body: &B,
    ) -> Result<T, BankSyncError> {
        let token = self.access_token().await?;
        let resp = self
            .http
            .post(url)
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let resp = Self::check_response(resp).await?;
        resp.json()
            .await
            .map_err(|e| BankSyncError::Parse(e.to_string()))
    }

    async fn authorized_delete(&self, url: String) -> Result<(), BankSyncError> {
        let token = self.access_token().await?;
        let resp = self
            .http
            .delete(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        Self::check_response(resp).await?;
        Ok(())
    }

    async fn check_response(resp: reqwest::Response) -> Result<reqwest::Response, BankSyncError> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }

        let body = resp.text().await.unwrap_or_default();
        match status.as_u16() {
            401 | 403 => Err(BankSyncError::Auth(body)),
            404 => Err(BankSyncError::NotFound(body)),
            429 => Err(BankSyncError::RateLimit),
            _ => Err(BankSyncError::Provider(format!("HTTP {status}: {body}"))),
        }
    }
}

#[derive(Serialize)]
struct TokenNewRequest<'a> {
    secret_id: &'a str,
    secret_key: &'a str,
}

#[derive(Deserialize)]
struct TokenNewResponse {
    refresh: String,
}

#[derive(Serialize)]
struct TokenRefreshRequest<'a> {
    refresh: &'a str,
}

#[derive(Deserialize)]
struct TokenRefreshResponse {
    access: String,
    access_expires: u64,
}

#[derive(Deserialize)]
struct InstitutionEntry {
    id: String,
    name: String,
    #[serde(default)]
    logo: Option<String>,
    #[serde(default)]
    countries: Vec<String>,
}

#[derive(Serialize)]
struct RequisitionRequest<'a> {
    redirect: &'a str,
    institution_id: &'a str,
    reference: &'a str,
}

#[derive(Deserialize)]
struct RequisitionResponse {
    id: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    accounts: Vec<String>,
}

#[derive(Deserialize)]
struct AccountDetailsResponse {
    account: AccountDetails,
}

#[derive(Deserialize)]
struct AccountDetails {
    #[serde(default, rename = "displayName")]
    display_name: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    iban: Option<String>,
    currency: String,
}

#[derive(Deserialize)]
struct BalancesResponse {
    balances: Vec<BalanceEntry>,
}

#[derive(Deserialize)]
struct BalanceEntry {
    #[serde(rename = "balanceAmount")]
    balance_amount: AmountValue,
    #[serde(rename = "balanceType")]
    balance_type: String,
}

#[derive(Deserialize, Clone)]
struct AmountValue {
    amount: String,
    currency: String,
}

#[derive(Deserialize)]
struct TransactionsResponse {
    transactions: TransactionLists,
}

#[derive(Deserialize)]
struct TransactionLists {
    #[serde(default)]
    booked: Vec<GoCardlessTransaction>,
}

#[derive(Deserialize)]
struct GoCardlessTransaction {
    #[serde(default, rename = "transactionId")]
    transaction_id: Option<String>,
    #[serde(default, rename = "internalTransactionId")]
    internal_transaction_id: Option<String>,
    #[serde(default, rename = "entryReference")]
    entry_reference: Option<String>,
    #[serde(rename = "transactionAmount")]
    transaction_amount: AmountValue,
    #[serde(default, rename = "bookingDate")]
    booking_date: Option<String>,
    #[serde(default, rename = "valueDate")]
    value_date: Option<String>,
    #[serde(default, rename = "remittanceInformationUnstructured")]
    remittance_information_unstructured: Option<String>,
    #[serde(default, rename = "remittanceInformationUnstructuredArray")]
    remittance_information_unstructured_array: Option<Vec<String>>,
    #[serde(default, rename = "additionalInformation")]
    additional_information: Option<String>,
    #[serde(default, rename = "endToEndId")]
    end_to_end_id: Option<String>,
}

impl GoCardlessTransaction {
    fn stable_id(&self) -> Option<String> {
        self.transaction_id
            .clone()
            .or_else(|| self.internal_transaction_id.clone())
            .or_else(|| self.entry_reference.clone())
    }

    fn date(&self) -> Option<NaiveDate> {
        self.booking_date
            .as_deref()
            .or(self.value_date.as_deref())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    }

    fn description(&self) -> String {
        if let Some(values) = self.remittance_information_unstructured_array.as_ref() {
            if !values.is_empty() {
                return values.join(" / ");
            }
        }

        self.remittance_information_unstructured
            .clone()
            .or_else(|| self.additional_information.clone())
            .unwrap_or_default()
    }
}

impl BankSyncProvider for GoCardlessProvider {
    fn provider_id(&self) -> &str {
        "gocardless"
    }

    fn display_name(&self) -> &str {
        "GoCardless Bank Account Data"
    }

    async fn list_aspsps(&self, country: Option<&str>) -> Result<Vec<AspspInfo>, BankSyncError> {
        let country = country.ok_or_else(|| {
            BankSyncError::Provider(
                "GoCardless requires a country filter to list institutions".into(),
            )
        })?;

        let entries: Vec<InstitutionEntry> = self
            .authorized_get(format!(
                "{}/institutions/?country={}",
                self.base_url, country
            ))
            .await?;

        Ok(entries
            .into_iter()
            .map(|entry| AspspInfo {
                institution_id: Some(entry.id),
                name: entry.name,
                country: entry
                    .countries
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| country.to_string()),
                logo_url: entry.logo,
            })
            .collect())
    }

    async fn start_authorization(
        &self,
        redirect_url: &str,
        state: &str,
        aspsp_name: &str,
        _aspsp_country: &str,
    ) -> Result<String, BankSyncError> {
        let data: RequisitionResponse = self
            .authorized_post(
                format!("{}/requisitions/", self.base_url),
                &RequisitionRequest {
                    redirect: redirect_url,
                    institution_id: aspsp_name,
                    reference: state,
                },
            )
            .await?;

        Ok(format!("{}|{}", data.id, data.link))
    }

    async fn complete_authorization(
        &self,
        code: &str,
    ) -> Result<(String, Vec<ProviderBankAccount>), BankSyncError> {
        let requisition: RequisitionResponse = self
            .authorized_get(format!("{}/requisitions/{code}/", self.base_url))
            .await?;

        let mut accounts = Vec::new();
        for account_id in requisition.accounts {
            let details: AccountDetailsResponse = self
                .authorized_get(format!("{}/accounts/{account_id}/details/", self.base_url))
                .await?;

            accounts.push(ProviderBankAccount {
                uid: account_id,
                name: details.account.display_name.or(details.account.name),
                iban: details.account.iban,
                currency: details.account.currency,
            });
        }

        Ok((code.to_string(), accounts))
    }

    async fn get_balances(
        &self,
        _session_id: &str,
        account_uid: &str,
    ) -> Result<Vec<ProviderBalance>, BankSyncError> {
        let data: BalancesResponse = self
            .authorized_get(format!(
                "{}/accounts/{account_uid}/balances/",
                self.base_url
            ))
            .await?;

        Ok(data
            .balances
            .into_iter()
            .filter_map(|balance| {
                let amount = Decimal::from_str(&balance.balance_amount.amount).ok()?;
                Some(ProviderBalance {
                    amount,
                    currency: balance.balance_amount.currency,
                    balance_type: balance.balance_type,
                })
            })
            .collect())
    }

    async fn fetch_transactions(
        &self,
        _session_id: &str,
        account_uid: &str,
        since: Option<NaiveDate>,
    ) -> Result<Vec<ProviderTransaction>, BankSyncError> {
        let mut url = format!("{}/accounts/{account_uid}/transactions/", self.base_url);
        if let Some(date) = since {
            url.push_str(&format!("?date_from={}", date.format("%Y-%m-%d")));
        }

        let data: TransactionsResponse = self.authorized_get(url).await?;

        let mut results = Vec::new();
        for (i, txn) in data.transactions.booked.into_iter().enumerate() {
            let external_id = txn
                .stable_id()
                .unwrap_or_else(|| format!("{account_uid}_{i}"));
            let Some(date) = txn.date() else {
                continue;
            };
            let Ok(amount) = Decimal::from_str(&txn.transaction_amount.amount) else {
                continue;
            };

            results.push(ProviderTransaction {
                external_id,
                date,
                amount,
                currency: txn.transaction_amount.currency.clone(),
                description: txn.description(),
                reference: txn.end_to_end_id,
            });
        }

        Ok(results)
    }

    async fn revoke_session(&self, session_id: &str) -> Result<(), BankSyncError> {
        self.authorized_delete(format!("{}/requisitions/{session_id}/", self.base_url))
            .await
    }
}
