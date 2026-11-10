use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::StorageResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankAuthState {
    pub user_id: Uuid,
    pub provider_id: String,
    pub state: String,
    pub aspsp_name: Option<String>,
    pub aspsp_country: Option<String>,
    pub provider_reference: Option<String>,
    pub source_bank_account_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait BankAuthStateRepository: Send + Sync {
    async fn create(&self, state: BankAuthState) -> StorageResult<()>;

    /// Removes and returns the user's unexpired state, so it can be used only once.
    async fn consume(&self, user_id: Uuid, state: &str) -> StorageResult<Option<BankAuthState>>;
}
