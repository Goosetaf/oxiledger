//! Application code depends on the repository traits here and never on the database library
//! behind them. There is one repository per aggregate.

pub mod accounts;
pub mod bank_accounts;
pub mod bank_transactions;
mod diesel_store;
mod error;
pub mod sessions;
pub mod transactions;
pub mod users;

use std::sync::Arc;

pub use error::{StorageError, StorageResult};

pub const DEFAULT_DATABASE_URL: &str = "sqlite://data/oxiledger.db";

#[derive(Clone)]
pub struct Storage {
    pub users: Arc<dyn users::UserRepository>,
    pub sessions: Arc<dyn sessions::SessionRepository>,
    pub accounts: Arc<dyn accounts::AccountRepository>,
    pub transactions: Arc<dyn transactions::TransactionRepository>,
    pub bank_accounts: Arc<dyn bank_accounts::BankAccountRepository>,
    pub bank_transactions: Arc<dyn bank_transactions::BankTransactionRepository>,
}

impl Storage {
    pub async fn connect(database_url: &str) -> StorageResult<Self> {
        diesel_store::connect(database_url).await
    }
}

#[cfg(test)]
mod tests;
