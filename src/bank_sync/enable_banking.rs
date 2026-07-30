/// Enable Banking provider implementation.
///
/// Uses the Enable Banking REST API (https://api.enablebanking.com).
/// Authentication uses RS256 JWTs signed with a private RSA key.
///
/// Required environment variables:
/// - `ENABLE_BANKING_APP_ID`          — application ID from the Enable Banking control panel.
/// - `ENABLE_BANKING_PRIVATE_KEY_PATH`— path to the PEM-encoded RSA private key file.
/// - `ENABLE_BANKING_ENVIRONMENT`     — `SANDBOX` (default) or `PRODUCTION`.
use chrono::NaiveDate;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use super::{
    error::BankSyncError,
    provider::{
        AspspInfo, BankSyncProvider, ProviderBalance, ProviderBankAccount, ProviderTransaction,
    },
};

const SANDBOX_BASE_URL: &str = "https://api.enablebanking.com";
const PRODUCTION_BASE_URL: &str = "https://api.enablebanking.com";

/// JWT claims sent with every Enable Banking API request.
#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,
    aud: String,
    iat: u64,
    exp: u64,
}

pub struct EnableBankingProvider {
    app_id: String,
    encoding_key: EncodingKey,
    base_url: String,
    http: Client,
}

impl EnableBankingProvider {
    /// Construct from environment variables.
    ///
    /// Returns `Err(BankSyncError::Config)` if any required variable is absent or invalid.
    pub fn from_env() -> Result<Self, BankSyncError> {
        let app_id = std::env::var("ENABLE_BANKING_APP_ID")
            .map_err(|_| BankSyncError::Config("ENABLE_BANKING_APP_ID is not set".into()))?;

        let key_path = std::env::var("ENABLE_BANKING_PRIVATE_KEY_PATH").map_err(|_| {
            BankSyncError::Config("ENABLE_BANKING_PRIVATE_KEY_PATH is not set".into())
        })?;

        let pem = std::fs::read(&key_path).map_err(|e| {
            BankSyncError::Config(format!("Cannot read private key at {key_path}: {e}"))
        })?;

        let encoding_key = EncodingKey::from_rsa_pem(&pem)
            .map_err(|e| BankSyncError::Config(format!("Invalid RSA private key: {e}")))?;

        let env = std::env::var("ENABLE_BANKING_ENVIRONMENT").unwrap_or_else(|_| "SANDBOX".into());
        let base_url = if env.eq_ignore_ascii_case("PRODUCTION") {
            PRODUCTION_BASE_URL.to_string()
        } else {
            SANDBOX_BASE_URL.to_string()
        };

        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        Ok(Self {
            app_id,
            encoding_key,
            base_url,
            http,
        })
    }

    pub fn is_configured() -> bool {
        std::env::var("ENABLE_BANKING_APP_ID").is_ok()
            && std::env::var("ENABLE_BANKING_PRIVATE_KEY_PATH").is_ok()
    }

    /// Build a short-lived JWT (5-minute TTL) for a single API request.
    fn make_jwt(&self) -> Result<String, BankSyncError> {
        let now = jsonwebtoken::get_current_timestamp();
        let claims = JwtClaims {
            iss: "enablebanking.com".into(),
            aud: "api.enablebanking.com".into(),
            iat: now,
            exp: now + 300, // 5 minutes
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.app_id.clone());

        encode(&header, &claims, &self.encoding_key)
            .map_err(|e| BankSyncError::Auth(format!("JWT signing failed: {e}")))
    }

    /// Common error mapping from HTTP status codes.
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

// ── Enable Banking API response types ─────────────────────────────────────────

#[derive(Deserialize)]
struct AspspListResponse {
    aspsps: Vec<AspspEntry>,
}

#[derive(Deserialize)]
struct AspspEntry {
    name: String,
    country: String,
    #[serde(default)]
    logo: Option<String>,
}

#[derive(Serialize)]
struct StartAuthRequest<'a> {
    access: AccessSpec,
    aspsp: AspspRef<'a>,
    state: &'a str,
    redirect_url: &'a str,
}

#[derive(Serialize)]
struct AccessSpec {
    valid_until: String,
}

#[derive(Serialize)]
struct AspspRef<'a> {
    name: &'a str,
    country: &'a str,
}

#[derive(Deserialize)]
struct StartAuthResponse {
    url: String,
}

#[derive(Serialize)]
struct CompleteAuthRequest<'a> {
    code: &'a str,
}

#[derive(Deserialize)]
struct SessionAccount {
    uid: Option<String>,
    #[serde(default)]
    account_id: Option<AccountId>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    currency: Option<String>,
}

#[derive(Deserialize)]
struct AccountId {
    #[serde(default)]
    iban: Option<String>,
}

#[derive(Deserialize)]
struct CompleteAuthResponse {
    session_id: String,
    #[serde(default)]
    accounts: Vec<SessionAccount>,
}

#[derive(Deserialize)]
struct BalancesResponse {
    balances: Vec<BalanceEntry>,
}

#[derive(Deserialize)]
struct BalanceEntry {
    balance_amount: AmountValue,
    balance_type: Option<String>,
}

#[derive(Deserialize)]
struct AmountValue {
    amount: String,
    currency: String,
}

#[derive(Deserialize)]
struct TransactionsResponse {
    #[serde(default)]
    transactions: Vec<EnableBankingTransaction>,
}

#[derive(Deserialize)]
struct EnableBankingTransaction {
    #[serde(default)]
    transaction_id: Option<String>,
    #[serde(default)]
    entry_reference: Option<String>,
    transaction_amount: AmountValue,
    #[serde(default)]
    credit_debit_indicator: Option<String>, // "CRDT" or "DBIT"
    #[serde(default)]
    booking_date: Option<String>,
    #[serde(default)]
    value_date: Option<String>,
    #[serde(default)]
    remittance_information: Vec<String>,
    #[serde(default)]
    reference_number: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

impl EnableBankingTransaction {
    fn stable_id(&self) -> Option<String> {
        self.transaction_id
            .clone()
            .or_else(|| self.entry_reference.clone())
    }

    fn date(&self) -> Option<NaiveDate> {
        self.booking_date
            .as_deref()
            .or(self.value_date.as_deref())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    }

    fn signed_amount(&self) -> Option<Decimal> {
        let raw = Decimal::from_str(&self.transaction_amount.amount).ok()?;
        // DBIT = outflow → negative; CRDT = inflow → positive; no indicator → use sign.
        let signed = match self.credit_debit_indicator.as_deref() {
            Some("DBIT") => -raw.abs(),
            Some("CRDT") => raw.abs(),
            _ => raw, // trust the raw sign if provided
        };
        Some(signed)
    }

    fn description(&self) -> String {
        if !self.remittance_information.is_empty() {
            self.remittance_information.join(" / ")
        } else if let Some(note) = &self.note {
            note.clone()
        } else {
            String::new()
        }
    }
}

// ── Trait implementation ───────────────────────────────────────────────────────

impl BankSyncProvider for EnableBankingProvider {
    fn provider_id(&self) -> &str {
        "enable_banking"
    }

    fn display_name(&self) -> &str {
        "Enable Banking"
    }

    async fn list_aspsps(&self, country: Option<&str>) -> Result<Vec<AspspInfo>, BankSyncError> {
        let jwt = self.make_jwt()?;
        let mut url = format!("{}/aspsps", self.base_url);
        if let Some(c) = country {
            url.push_str(&format!("?country={c}"));
        }

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let resp = Self::check_response(resp).await?;
        let data: AspspListResponse = resp
            .json()
            .await
            .map_err(|e| BankSyncError::Parse(e.to_string()))?;

        Ok(data
            .aspsps
            .into_iter()
            .map(|a| AspspInfo {
                institution_id: None,
                name: a.name,
                country: a.country,
                logo_url: a.logo,
            })
            .collect())
    }

    async fn start_authorization(
        &self,
        redirect_url: &str,
        state: &str,
        aspsp_name: &str,
        aspsp_country: &str,
    ) -> Result<String, BankSyncError> {
        let jwt = self.make_jwt()?;

        // Request access valid for 90 days from now.
        let valid_until = (chrono::Utc::now() + chrono::Duration::days(90))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();

        let body = StartAuthRequest {
            access: AccessSpec { valid_until },
            aspsp: AspspRef {
                name: aspsp_name,
                country: aspsp_country,
            },
            state,
            redirect_url,
        };

        let resp = self
            .http
            .post(&format!("{}/auth", self.base_url))
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let resp = Self::check_response(resp).await?;
        let data: StartAuthResponse = resp
            .json()
            .await
            .map_err(|e| BankSyncError::Parse(e.to_string()))?;

        Ok(data.url)
    }

    async fn complete_authorization(
        &self,
        code: &str,
    ) -> Result<(String, Vec<ProviderBankAccount>), BankSyncError> {
        let jwt = self.make_jwt()?;
        let body = CompleteAuthRequest { code };

        let resp = self
            .http
            .post(&format!("{}/sessions", self.base_url))
            .bearer_auth(&jwt)
            .json(&body)
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let resp = Self::check_response(resp).await?;
        let data: CompleteAuthResponse = resp
            .json()
            .await
            .map_err(|e| BankSyncError::Parse(e.to_string()))?;

        let accounts = data
            .accounts
            .into_iter()
            .filter_map(|a| {
                // uid is required — skip accounts without it.
                let uid = a.uid?;
                Some(ProviderBankAccount {
                    iban: a.account_id.and_then(|id| id.iban),
                    name: a.name,
                    currency: a.currency.unwrap_or_else(|| "EUR".into()),
                    uid,
                })
            })
            .collect();

        Ok((data.session_id, accounts))
    }

    async fn get_balances(
        &self,
        session_id: &str,
        account_uid: &str,
    ) -> Result<Vec<ProviderBalance>, BankSyncError> {
        let jwt = self.make_jwt()?;
        let url = format!("{}/accounts/{}/balances", self.base_url, account_uid);

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&jwt)
            .header("Session-Id", session_id)
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let resp = Self::check_response(resp).await?;
        let data: BalancesResponse = resp
            .json()
            .await
            .map_err(|e| BankSyncError::Parse(e.to_string()))?;

        let balances = data
            .balances
            .into_iter()
            .filter_map(|b| {
                let amount = Decimal::from_str(&b.balance_amount.amount).ok()?;
                Some(ProviderBalance {
                    amount,
                    currency: b.balance_amount.currency,
                    balance_type: b.balance_type.unwrap_or_else(|| "UNKNOWN".into()),
                })
            })
            .collect();

        Ok(balances)
    }

    async fn fetch_transactions(
        &self,
        session_id: &str,
        account_uid: &str,
        since: Option<NaiveDate>,
    ) -> Result<Vec<ProviderTransaction>, BankSyncError> {
        let jwt = self.make_jwt()?;
        let mut url = format!(
            "{}/accounts/{}/transactions?transaction_status=BOOK",
            self.base_url, account_uid
        );
        if let Some(date) = since {
            url.push_str(&format!("&date_from={}", date.format("%Y-%m-%d")));
        }

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&jwt)
            .header("Session-Id", session_id)
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        let resp = Self::check_response(resp).await?;
        let data: TransactionsResponse = resp
            .json()
            .await
            .map_err(|e| BankSyncError::Parse(e.to_string()))?;

        let mut results = Vec::new();
        for (i, t) in data.transactions.into_iter().enumerate() {
            let external_id = match t.stable_id() {
                Some(id) => id,
                None => {
                    // Generate a deterministic fallback ID so we can still import.
                    format!("{}_{}", account_uid, i)
                }
            };
            let date = match t.date() {
                Some(d) => d,
                None => continue, // skip transactions with no parseable date
            };
            let amount = match t.signed_amount() {
                Some(a) => a,
                None => continue,
            };
            results.push(ProviderTransaction {
                external_id,
                date,
                amount,
                currency: t.transaction_amount.currency.clone(),
                description: t.description(),
                reference: t.reference_number.clone(),
            });
        }

        Ok(results)
    }

    async fn revoke_session(&self, session_id: &str) -> Result<(), BankSyncError> {
        let jwt = self.make_jwt()?;
        let url = format!("{}/sessions/{}", self.base_url, session_id);

        let resp = self
            .http
            .delete(&url)
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| BankSyncError::Http(e.to_string()))?;

        Self::check_response(resp).await?;
        Ok(())
    }
}
