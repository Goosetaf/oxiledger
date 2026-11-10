use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::StorageResult;
use crate::models::bank_sync::{
    BankTransaction, BankTransactionOverview, BankTransactionPrefill, SyncResult,
};

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

#[async_trait]
pub trait BankTransactionRepository: Send + Sync {
    async fn list_pending(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
    ) -> StorageResult<Vec<BankTransaction>>;

    async fn list_overview(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
    ) -> StorageResult<Vec<BankTransactionOverview>>;

    async fn create(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
        transaction: NewBankTransaction,
    ) -> StorageResult<()>;

    /// Stores the transactions not seen before and records the account as synced just now.
    async fn import(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
        transactions: Vec<NewBankTransaction>,
        balance: Option<Decimal>,
    ) -> StorageResult<SyncResult>;

    async fn dismiss(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn restore(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

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

    async fn link_info(
        &self,
        user_id: Uuid,
        ids: Vec<Uuid>,
    ) -> StorageResult<Vec<BankTransactionLinkInfo>>;
}
