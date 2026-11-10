//! The persistence boundary. Application code depends on the repository traits defined here,
//! never on the database library behind them.

pub mod accounts;
pub mod bank_accounts;
pub mod bank_auth_states;
pub mod bank_connections;
pub mod bank_transactions;
mod diesel_store;
mod error;
pub mod oidc_states;
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
    pub oidc_states: Arc<dyn oidc_states::OidcStateRepository>,
    pub accounts: Arc<dyn accounts::AccountRepository>,
    pub transactions: Arc<dyn transactions::TransactionRepository>,
    pub bank_accounts: Arc<dyn bank_accounts::BankAccountRepository>,
    pub bank_connections: Arc<dyn bank_connections::BankConnectionRepository>,
    pub bank_auth_states: Arc<dyn bank_auth_states::BankAuthStateRepository>,
    pub bank_transactions: Arc<dyn bank_transactions::BankTransactionRepository>,
}

impl Storage {
    /// Opens the database behind `database_url` and brings its schema up to date.
    pub async fn connect(database_url: &str) -> StorageResult<Self> {
        diesel_store::connect(database_url).await
    }
}

#[cfg(test)]
mod tests;
