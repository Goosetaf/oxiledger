use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::StorageResult;
use crate::bank_sync::provider::ProviderBankAccount;

#[derive(Debug, Clone)]
pub struct NewBankConnection {
    pub user_id: Uuid,
    pub provider_id: String,
    pub provider_session_id: String,
    pub aspsp_name: Option<String>,
    pub aspsp_country: Option<String>,
    pub access_valid_until: DateTime<Utc>,
    pub provider_accounts: Vec<ProviderBankAccount>,
    pub source_bank_account_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct BankConnectionDetails {
    /// The manual bank account the connection was started from, if any.
    pub source_bank_account_id: Option<Uuid>,
    pub provider_accounts: Vec<ProviderBankAccount>,
}

#[async_trait]
pub trait BankConnectionRepository: Send + Sync {
    async fn create(&self, connection: NewBankConnection) -> StorageResult<Uuid>;

    async fn find(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<BankConnectionDetails>>;

    async fn delete(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;
}
