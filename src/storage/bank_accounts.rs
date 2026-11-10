use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::StorageResult;
use crate::models::bank_sync::{BankAccountRecord, BankAccountSummary};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankAccountDetails {
    pub name: String,
    pub iban: Option<String>,
    pub currency: String,
    pub internal_account_id: Option<Uuid>,
}

/// An account as a provider describes it.
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

/// What syncing a bank account needs to know about it.
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

#[async_trait]
pub trait BankAccountRepository: Send + Sync {
    /// Ordered by bank name, then account name.
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

    /// Adds a provider's account to a connection, or re-maps it if it was added before.
    async fn map_provider_account(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        account: ProviderAccount,
        internal_account_id: Option<Uuid>,
    ) -> StorageResult<Uuid>;

    /// Turns an existing bank account into a connected one. Sync state starts over.
    async fn connect_to_provider(
        &self,
        user_id: Uuid,
        id: Uuid,
        connection_id: Uuid,
        account: ProviderAccount,
    ) -> StorageResult<bool>;

    /// Turns a connected bank account into a manual one.
    async fn disconnect_from_provider(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    /// With `user_id` set, only finds an account that user owns.
    async fn find_syncable(
        &self,
        id: Uuid,
        user_id: Option<Uuid>,
    ) -> StorageResult<Option<SyncableBankAccount>>;

    /// Every connected bank account, across all users.
    async fn list_sync_candidates(&self) -> StorageResult<Vec<SyncCandidate>>;

    /// Which of the provider account UIDs the user already has outside the given connection.
    async fn linked_provider_uids(
        &self,
        user_id: Uuid,
        uids: Vec<String>,
        outside_connection_id: Uuid,
    ) -> StorageResult<Vec<String>>;
}
