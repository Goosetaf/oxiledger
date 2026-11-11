use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::StorageResult;
use crate::bank_sync::provider::ProviderBankAccount;
use crate::models::bank_sync::{
    BankAccountRecord, BankAccountSummary, BankTransaction, BankTransactionOverview,
    BankTransactionPrefill, SyncResult,
};

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

/// What is kept while a user is away authorizing a connection at their bank.
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

#[derive(Debug, Clone, PartialEq)]
pub struct NewBankTransaction {
    pub external_id: String,
    pub date: NaiveDate,
    pub amount: Decimal,
    pub currency: String,
    pub description: String,
    pub reference: Option<String>,
}

/// What deciding whether a bank transaction may be linked to a journal entry needs.
#[derive(Debug, Clone, PartialEq)]
pub struct BankTransactionLinkInfo {
    pub id: Uuid,
    pub date: NaiveDate,
    pub amount: Decimal,
    pub status: String,
    pub internal_account_id: Option<Uuid>,
    /// The journal transaction it is linked to now, if any.
    pub linked_transaction_id: Option<Uuid>,
}

/// Bank sync: bank accounts, the provider connections behind them, and the transactions
/// imported from them.
#[async_trait]
pub trait BankRepository: Send + Sync {
    /// Ordered by bank name, then account name.
    async fn list_accounts(&self, user_id: Uuid) -> StorageResult<Vec<BankAccountSummary>>;

    async fn find_account_summary(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<BankAccountSummary>>;

    async fn find_account(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<BankAccountRecord>>;

    async fn list_connection_accounts(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
    ) -> StorageResult<Vec<BankAccountRecord>>;

    async fn count_connection_accounts(&self, connection_id: Uuid) -> StorageResult<i64>;

    async fn create_manual_account(
        &self,
        user_id: Uuid,
        details: BankAccountDetails,
    ) -> StorageResult<Uuid>;

    async fn update_account(
        &self,
        user_id: Uuid,
        id: Uuid,
        details: BankAccountDetails,
    ) -> StorageResult<bool>;

    async fn delete_account(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    /// Adds a provider's account to a connection, or re-maps it if it was added before.
    async fn map_provider_account(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        account: ProviderAccount,
        internal_account_id: Option<Uuid>,
    ) -> StorageResult<Uuid>;

    /// Turns an existing bank account into a connected one. Sync state starts over.
    async fn connect_account(
        &self,
        user_id: Uuid,
        id: Uuid,
        connection_id: Uuid,
        account: ProviderAccount,
    ) -> StorageResult<bool>;

    /// Turns a connected bank account into a manual one.
    async fn disconnect_account(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    /// With `user_id` set, only finds an account that user owns.
    async fn find_syncable_account(
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

    async fn create_connection(&self, connection: NewBankConnection) -> StorageResult<Uuid>;

    async fn find_connection(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<BankConnectionDetails>>;

    async fn delete_connection(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn create_auth_state(&self, state: BankAuthState) -> StorageResult<()>;

    /// Removes and returns the user's unexpired state, so it can be used only once.
    async fn consume_auth_state(
        &self,
        user_id: Uuid,
        state: &str,
    ) -> StorageResult<Option<BankAuthState>>;

    async fn list_pending_transactions(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
    ) -> StorageResult<Vec<BankTransaction>>;

    async fn list_transaction_overview(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
    ) -> StorageResult<Vec<BankTransactionOverview>>;

    async fn create_transaction(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
        transaction: NewBankTransaction,
    ) -> StorageResult<()>;

    /// Stores the transactions not seen before and records the account as synced just now.
    async fn import_transactions(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
        transactions: Vec<NewBankTransaction>,
        balance: Option<Decimal>,
    ) -> StorageResult<SyncResult>;

    async fn dismiss_transaction(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn restore_transaction(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn find_pending_prefill(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<BankTransactionPrefill>>;

    async fn list_pending_prefills(
        &self,
        user_id: Uuid,
        date: NaiveDate,
    ) -> StorageResult<Vec<BankTransactionPrefill>>;

    /// Pending transactions on the user's other bank accounts that mirror this one: same date,
    /// opposite amount.
    async fn list_transfer_matches(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Vec<BankTransactionPrefill>>;

    async fn transaction_link_info(
        &self,
        user_id: Uuid,
        ids: Vec<Uuid>,
    ) -> StorageResult<Vec<BankTransactionLinkInfo>>;
}
