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

/// Writes are atomic. An entry naming a bank transaction links it, and fails with
/// `StorageError::Conflict` unless that bank transaction is pending or already linked to the
/// transaction being updated.
#[async_trait]
pub trait TransactionRepository: Send + Sync {
    /// Newest first, each with its entries.
    async fn list(&self, user_id: Uuid) -> StorageResult<Vec<Transaction>>;

    async fn find(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<Transaction>>;

    async fn create(&self, user_id: Uuid, draft: TransactionDraft) -> StorageResult<Uuid>;

    /// Replaces the entries. Bank transactions no longer named go back to pending.
    async fn update(&self, user_id: Uuid, id: Uuid, draft: TransactionDraft)
        -> StorageResult<bool>;

    /// Linked bank transactions go back to pending.
    async fn delete(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;
}
