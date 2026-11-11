use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::StorageResult;
use crate::models::{
    account::{Account, AccountType, NormalBalance},
    transaction::{AccountBalance, JournalEntryInput, Transaction},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDraft {
    pub name: String,
    pub code: Option<String>,
    pub account_type: AccountType,
    pub normal_balance: NormalBalance,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionDraft {
    pub date: NaiveDate,
    pub description: String,
    pub reference: Option<String>,
    pub entries: Vec<JournalEntryInput>,
}

/// The double-entry ledger: the chart of accounts and the transactions posted to it.
///
/// Account lists are ordered by account type, then code, then name.
///
/// Transaction writes are atomic. An entry naming a bank transaction links it, and fails with
/// `StorageError::Conflict` unless that bank transaction is pending or already linked to the
/// transaction being updated.
#[async_trait]
pub trait LedgerRepository: Send + Sync {
    async fn list_accounts(&self, user_id: Uuid) -> StorageResult<Vec<Account>>;

    async fn list_active_accounts(&self, user_id: Uuid) -> StorageResult<Vec<Account>>;

    async fn find_account(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<Account>>;

    async fn create_account(&self, user_id: Uuid, draft: AccountDraft) -> StorageResult<Account>;

    async fn update_account(
        &self,
        user_id: Uuid,
        id: Uuid,
        draft: AccountDraft,
    ) -> StorageResult<bool>;

    async fn toggle_account_active(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn delete_account(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    /// Debits minus credits for each active account.
    async fn account_balances(&self, user_id: Uuid) -> StorageResult<Vec<AccountBalance>>;

    /// Debits minus credits posted to one account.
    async fn account_balance(&self, user_id: Uuid, id: Uuid) -> StorageResult<Decimal>;

    /// Newest first, each with its entries.
    async fn list_transactions(&self, user_id: Uuid) -> StorageResult<Vec<Transaction>>;

    async fn find_transaction(&self, user_id: Uuid, id: Uuid)
        -> StorageResult<Option<Transaction>>;

    async fn create_transaction(
        &self,
        user_id: Uuid,
        draft: TransactionDraft,
    ) -> StorageResult<Uuid>;

    /// Replaces the entries. Bank transactions no longer named go back to pending.
    async fn update_transaction(
        &self,
        user_id: Uuid,
        id: Uuid,
        draft: TransactionDraft,
    ) -> StorageResult<bool>;

    /// Linked bank transactions go back to pending.
    async fn delete_transaction(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;
}
