use async_trait::async_trait;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::StorageResult;
use crate::models::{
    account::{Account, AccountType, NormalBalance},
    transaction::AccountBalance,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDraft {
    pub name: String,
    pub code: Option<String>,
    pub account_type: AccountType,
    pub normal_balance: NormalBalance,
    pub description: Option<String>,
}

#[async_trait]
pub trait AccountRepository: Send + Sync {
    async fn list(&self, user_id: Uuid) -> StorageResult<Vec<Account>>;

    async fn list_active(&self, user_id: Uuid) -> StorageResult<Vec<Account>>;

    async fn find(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<Account>>;

    async fn create(&self, user_id: Uuid, draft: AccountDraft) -> StorageResult<Account>;

    async fn update(&self, user_id: Uuid, id: Uuid, draft: AccountDraft) -> StorageResult<bool>;

    async fn toggle_active(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn delete(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool>;

    async fn balances(&self, user_id: Uuid) -> StorageResult<Vec<AccountBalance>>;

    async fn balance(&self, user_id: Uuid, id: Uuid) -> StorageResult<Decimal>;
}
