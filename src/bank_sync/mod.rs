use crate::bank_sync::provider::BankSyncProvider;

/// Bank sync subsystem.
///
/// This module provides a provider-agnostic abstraction for syncing bank
/// transactions. The [`provider::BankSyncProvider`] trait defines the interface
/// that every provider must implement. Add new providers by creating a new
/// module and implementing the trait — no other code needs to change.
///
/// Currently implemented providers:
/// - [`enable_banking`] — Enable Banking (https://enablebanking.com)
/// - [`gocardless`] — GoCardless Bank Account Data
pub mod error;
pub mod provider;

#[cfg(feature = "server")]
pub mod enable_banking;

#[cfg(feature = "server")]
pub mod gocardless;

#[cfg(feature = "server")]
pub use enable_banking::EnableBankingProvider;

#[cfg(feature = "server")]
pub use gocardless::GoCardlessProvider;

#[cfg(feature = "server")]
pub enum AnyBankSyncProvider {
    EnableBanking(EnableBankingProvider),
    GoCardless(GoCardlessProvider),
}

#[cfg(feature = "server")]
impl AnyBankSyncProvider {
    pub async fn list_aspsps(
        &self,
        country: Option<&str>,
    ) -> Result<Vec<provider::AspspInfo>, error::BankSyncError> {
        match self {
            AnyBankSyncProvider::EnableBanking(provider) => provider.list_aspsps(country).await,
            AnyBankSyncProvider::GoCardless(provider) => provider.list_aspsps(country).await,
        }
    }

    pub async fn start_authorization(
        &self,
        redirect_url: &str,
        state: &str,
        aspsp_name: &str,
        aspsp_country: &str,
    ) -> Result<String, error::BankSyncError> {
        match self {
            AnyBankSyncProvider::EnableBanking(provider) => {
                provider
                    .start_authorization(redirect_url, state, aspsp_name, aspsp_country)
                    .await
            }
            AnyBankSyncProvider::GoCardless(provider) => {
                provider
                    .start_authorization(redirect_url, state, aspsp_name, aspsp_country)
                    .await
            }
        }
    }

    pub async fn complete_authorization(
        &self,
        code: &str,
    ) -> Result<(String, Vec<provider::ProviderBankAccount>), error::BankSyncError> {
        match self {
            AnyBankSyncProvider::EnableBanking(provider) => {
                provider.complete_authorization(code).await
            }
            AnyBankSyncProvider::GoCardless(provider) => provider.complete_authorization(code).await,
        }
    }

    pub async fn get_balances(
        &self,
        session_id: &str,
        account_uid: &str,
    ) -> Result<Vec<provider::ProviderBalance>, error::BankSyncError> {
        match self {
            AnyBankSyncProvider::EnableBanking(provider) => {
                provider.get_balances(session_id, account_uid).await
            }
            AnyBankSyncProvider::GoCardless(provider) => {
                provider.get_balances(session_id, account_uid).await
            }
        }
    }

    pub async fn fetch_transactions(
        &self,
        session_id: &str,
        account_uid: &str,
        since: Option<chrono::NaiveDate>,
    ) -> Result<Vec<provider::ProviderTransaction>, error::BankSyncError> {
        match self {
            AnyBankSyncProvider::EnableBanking(provider) => {
                provider.fetch_transactions(session_id, account_uid, since).await
            }
            AnyBankSyncProvider::GoCardless(provider) => {
                provider.fetch_transactions(session_id, account_uid, since).await
            }
        }
    }

    pub async fn revoke_session(&self, session_id: &str) -> Result<(), error::BankSyncError> {
        match self {
            AnyBankSyncProvider::EnableBanking(provider) => provider.revoke_session(session_id).await,
            AnyBankSyncProvider::GoCardless(provider) => provider.revoke_session(session_id).await,
        }
    }
}

#[cfg(feature = "server")]
pub async fn get_provider(
    provider_id: &str,
) -> Result<AnyBankSyncProvider, error::BankSyncError> {
    match provider_id {
        "enable_banking" => Ok(AnyBankSyncProvider::EnableBanking(EnableBankingProvider::from_env()?)),
        "gocardless" => Ok(AnyBankSyncProvider::GoCardless(GoCardlessProvider::from_env().await?)),
        other => Err(error::BankSyncError::Provider(format!(
            "Unknown bank sync provider: {other}"
        ))),
    }
}
