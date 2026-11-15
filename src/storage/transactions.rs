use async_trait::async_trait;
use chrono::NaiveDate;
use uuid::Uuid;

use super::StorageResult;
use crate::models::transaction::{JournalEntryInput, Transaction};

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionDraft {
    pub date: NaiveDate,
    pub description: String,
    pub reference: Option<String>,
    pub entries: Vec<JournalEntryInput>,
}

/// An entry that names a bank transaction links it, and removing the entry returns it to
/// pending. A write fails with a conflict unless the named bank transaction is pending or
/// already linked to the transaction being saved.
#[async_trait]
pub trait TransactionRepository: Send + Sync {
    async fn list(&self, user_id: Uuid) -> StorageResult<Vec<Transaction>>;

    async fn find(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<Transaction>>;

    async fn create(&self, user_id: Uuid, draft: TransactionDraft) -> StorageResult<Uuid>;

    async fn update(&self, user_id: Uuid, id: Uuid, draft: TransactionDraft)
        -> StorageResult<bool>;

    async fn delete(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;
}
