use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::StorageResult;
use crate::bank_sync::provider::ProviderBankAccount;
use crate::models::bank_sync::{BankAccountRecord, BankAccountSummary};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankAccountDetails {
    pub name: String,
    pub iban: Option<String>,
    pub currency: String,
    pub internal_account_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAccount {
    pub uid: String,
    pub name: Option<String>,
    pub iban: Option<String>,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSession {
    pub connection_id: Uuid,
    pub provider_id: String,
    pub provider_session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncableBankAccount {
    pub id: Uuid,
    pub user_id: Uuid,
    pub is_manual: bool,
    pub provider_account_uid: Option<String>,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub session: Option<ProviderSession>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncCandidate {
    pub id: Uuid,
    pub last_synced_at: Option<DateTime<Utc>>,
}

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
    pub source_bank_account_id: Option<Uuid>,
    pub provider_accounts: Vec<ProviderBankAccount>,
}

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
pub trait BankAccountRepository: Send + Sync {
    async fn list(&self, user_id: Uuid) -> StorageResult<Vec<BankAccountSummary>>;

    async fn find_summary(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<BankAccountSummary>>;

    async fn find(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<BankAccountRecord>>;

    async fn list_for_connection(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
    ) -> StorageResult<Vec<BankAccountRecord>>;

    async fn count_for_connection(&self, connection_id: Uuid) -> StorageResult<i64>;

    async fn create_manual(
        &self,
        user_id: Uuid,
        details: BankAccountDetails,
    ) -> StorageResult<Uuid>;

    async fn update(
        &self,
        user_id: Uuid,
        id: Uuid,
        details: BankAccountDetails,
    ) -> StorageResult<bool>;

    async fn delete(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn map_provider_account(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        account: ProviderAccount,
        internal_account_id: Option<Uuid>,
    ) -> StorageResult<Uuid>;

    async fn connect_to_provider(
        &self,
        user_id: Uuid,
        id: Uuid,
        connection_id: Uuid,
        account: ProviderAccount,
    ) -> StorageResult<bool>;

    async fn disconnect_from_provider(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn find_syncable(
        &self,
        id: Uuid,
        user_id: Option<Uuid>,
    ) -> StorageResult<Option<SyncableBankAccount>>;

    async fn list_sync_candidates(&self) -> StorageResult<Vec<SyncCandidate>>;

    async fn linked_provider_uids(
        &self,
        user_id: Uuid,
        uids: Vec<String>,
        outside_connection_id: Uuid,
    ) -> StorageResult<Vec<String>>;
    async fn create_connection(&self, connection: NewBankConnection) -> StorageResult<Uuid>;

    async fn find_connection(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<BankConnectionDetails>>;

    async fn delete_connection(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn create_auth_state(&self, state: BankAuthState) -> StorageResult<()>;

    async fn consume_auth_state(
        &self,
        user_id: Uuid,
        state: &str,
    ) -> StorageResult<Option<BankAuthState>>;
}
