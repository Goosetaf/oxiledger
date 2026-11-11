//! The persistence boundary. Application code depends on the repository traits defined here,
//! never on the database library behind them. There is one repository per topic.

pub mod bank;
mod diesel_store;
mod error;
pub mod ledger;
pub mod users;

use std::sync::Arc;

pub use error::{StorageError, StorageResult};

pub const DEFAULT_DATABASE_URL: &str = "sqlite://data/oxiledger.db";

#[derive(Clone)]
pub struct Storage {
    pub users: Arc<dyn users::UserRepository>,
    pub ledger: Arc<dyn ledger::LedgerRepository>,
    pub bank: Arc<dyn bank::BankRepository>,
}

impl Storage {
    /// Opens the database behind `database_url` and brings its schema up to date.
    pub async fn connect(database_url: &str) -> StorageResult<Self> {
        diesel_store::connect(database_url).await
    }
}

#[cfg(test)]
mod tests;
