//! Repositories backed by Diesel, for SQLite and Postgres alike. Nothing outside this module
//! may depend on Diesel.

mod accounts;
mod bank_accounts;
mod bank_auth_states;
mod bank_connections;
mod bank_transactions;
mod connection;
mod migrations;
mod oidc_states;
mod schema;
mod sessions;
mod transactions;
mod types;
mod users;

#[cfg(test)]
pub mod test_support;

use std::sync::Arc;

use diesel::result::{DatabaseErrorKind, Error as DieselError};

use self::connection::Database;
use super::{Storage, StorageError, StorageResult};

diesel::define_sql_function! {
    fn coalesce(
        value: diesel::sql_types::Nullable<diesel::sql_types::Text>,
        fallback: diesel::sql_types::Text,
    ) -> diesel::sql_types::Text;
}

pub async fn connect(database_url: &str) -> StorageResult<Storage> {
    let database_url = database_url.to_string();
    let db = tokio::task::spawn_blocking(move || Database::open(&database_url))
        .await
        .map_err(|err| StorageError::Other(format!("Database task failed: {err}")))??;

    db.run(migrations::run).await?;

    Ok(Storage {
        users: Arc::new(users::UserStore::new(db.clone())),
        sessions: Arc::new(sessions::SessionStore::new(db.clone())),
        oidc_states: Arc::new(oidc_states::OidcStateStore::new(db.clone())),
        accounts: Arc::new(accounts::AccountStore::new(db.clone())),
        transactions: Arc::new(transactions::TransactionStore::new(db.clone())),
        bank_accounts: Arc::new(bank_accounts::BankAccountStore::new(db.clone())),
        bank_connections: Arc::new(bank_connections::BankConnectionStore::new(db.clone())),
        bank_auth_states: Arc::new(bank_auth_states::BankAuthStateStore::new(db.clone())),
        bank_transactions: Arc::new(bank_transactions::BankTransactionStore::new(db)),
    })
}

impl From<DieselError> for StorageError {
    fn from(err: DieselError) -> Self {
        match err {
            DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, info) => {
                StorageError::UniqueViolation(info.message().to_string())
            }
            DieselError::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, info) => {
                StorageError::ForeignKeyViolation(info.message().to_string())
            }
            other => StorageError::Other(other.to_string()),
        }
    }
}

macro_rules! store {
    ($name:ident) => {
        pub struct $name {
            db: $crate::storage::diesel_store::connection::Database,
        }

        impl $name {
            pub fn new(db: $crate::storage::diesel_store::connection::Database) -> Self {
                Self { db }
            }
        }
    };
}
use store;
