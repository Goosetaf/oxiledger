use std::path::Path;

use diesel::connection::{AnsiTransactionManager, SimpleConnection, TransactionManager};
use diesel::r2d2::{ManageConnection, Pool, R2D2Connection};
use diesel::{Connection, PgConnection, SqliteConnection};

use crate::storage::{StorageError, StorageResult};

#[derive(diesel::MultiConnection)]
pub enum DbConnection {
    Postgres(PgConnection),
    Sqlite(SqliteConnection),
}

/// Runs a query on the database's own connection type, for the features the shared query
/// builder leaves out, such as `ON CONFLICT`. The body is compiled once per database.
macro_rules! on_backend {
    ($conn:expr, |$c:ident| $body:expr) => {
        match $conn {
            $crate::storage::diesel_store::connection::DbConnection::Postgres($c) => $body,
            $crate::storage::diesel_store::connection::DbConnection::Sqlite($c) => $body,
        }
    };
}
pub(super) use on_backend;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Postgres(String),
    Sqlite(String),
}

impl Target {
    fn parse(database_url: &str) -> Self {
        if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
            return Target::Postgres(database_url.to_string());
        }

        let path = database_url
            .strip_prefix("sqlite://")
            .or_else(|| database_url.strip_prefix("sqlite:"))
            .unwrap_or(database_url);
        Target::Sqlite(path.to_string())
    }

    fn is_in_memory(&self) -> bool {
        matches!(self, Target::Sqlite(path) if path == ":memory:")
    }
}

#[derive(Debug)]
struct Manager {
    target: Target,
}

impl ManageConnection for Manager {
    type Connection = DbConnection;
    type Error = diesel::r2d2::Error;

    fn connect(&self) -> Result<DbConnection, Self::Error> {
        match &self.target {
            Target::Postgres(url) => PgConnection::establish(url)
                .map(DbConnection::Postgres)
                .map_err(diesel::r2d2::Error::ConnectionError),
            Target::Sqlite(path) => {
                let mut conn = SqliteConnection::establish(path)
                    .map_err(diesel::r2d2::Error::ConnectionError)?;
                // SQLite leaves foreign keys off, and fails writes at once when the file is busy.
                conn.batch_execute(
                    "PRAGMA foreign_keys = ON; \
                     PRAGMA busy_timeout = 5000; \
                     PRAGMA journal_mode = WAL; \
                     PRAGMA synchronous = NORMAL;",
                )
                .map_err(diesel::r2d2::Error::QueryError)?;
                Ok(DbConnection::Sqlite(conn))
            }
        }
    }

    fn is_valid(&self, conn: &mut DbConnection) -> Result<(), Self::Error> {
        conn.ping().map_err(diesel::r2d2::Error::QueryError)
    }

    fn has_broken(&self, conn: &mut DbConnection) -> bool {
        std::thread::panicking() || conn.is_broken()
    }
}

#[derive(Clone)]
pub struct Database {
    pool: Pool<Manager>,
}

impl Database {
    pub fn open(database_url: &str) -> StorageResult<Self> {
        let target = Target::parse(database_url);

        if let Target::Sqlite(path) = &target {
            if !target.is_in_memory() {
                create_parent_dir(path)?;
            }
        }

        // Every connection to an in-memory database gets a database of its own.
        let max_size = if target.is_in_memory() { 1 } else { 10 };

        let pool = Pool::builder()
            .max_size(max_size)
            .min_idle(Some(1))
            .build(Manager { target })
            .map_err(|err| StorageError::Other(format!("Failed to open the database: {err}")))?;

        Ok(Self { pool })
    }

    /// Runs blocking database work off the async runtime.
    pub async fn run<T, F>(&self, work: F) -> StorageResult<T>
    where
        F: FnOnce(&mut DbConnection) -> StorageResult<T> + Send + 'static,
        T: Send + 'static,
    {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = pool.get().map_err(|err| {
                StorageError::Other(format!("Failed to get a database connection: {err}"))
            })?;
            work(&mut conn)
        })
        .await
        .map_err(|err| StorageError::Other(format!("Database task failed: {err}")))?
    }

    /// Runs `work` in a transaction that commits if it returns `Ok` and rolls back otherwise.
    pub async fn transaction<T, F>(&self, work: F) -> StorageResult<T>
    where
        F: FnOnce(&mut DbConnection) -> StorageResult<T> + Send + 'static,
        T: Send + 'static,
    {
        self.run(move |conn| write_transaction(conn, work)).await
    }
}

fn create_parent_dir(path: &str) -> StorageResult<()> {
    let Some(parent) = Path::new(path).parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(parent).map_err(|err| {
        StorageError::Other(format!(
            "Failed to create the database directory {}: {err}",
            parent.display()
        ))
    })
}

pub(super) fn write_transaction<T>(
    conn: &mut DbConnection,
    work: impl FnOnce(&mut DbConnection) -> StorageResult<T>,
) -> StorageResult<T> {
    match conn {
        DbConnection::Postgres(pg) => AnsiTransactionManager::begin_transaction(pg)?,
        // Takes the write lock up front. A deferred transaction that reads first cannot wait
        // for another writer when it comes to write, and fails instead.
        DbConnection::Sqlite(sqlite) => {
            AnsiTransactionManager::begin_transaction_sql(sqlite, "BEGIN IMMEDIATE")?
        }
    }

    match work(conn) {
        Ok(value) => {
            <DbConnection as Connection>::TransactionManager::commit_transaction(conn)?;
            Ok(value)
        }
        Err(err) => {
            // A failed rollback leaves the connection broken, and the pool then discards it.
            let _ = <DbConnection as Connection>::TransactionManager::rollback_transaction(conn);
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_urls_select_the_database() {
        assert_eq!(
            Target::parse("postgres://user@host/db"),
            Target::Postgres("postgres://user@host/db".into())
        );
        assert_eq!(
            Target::parse("postgresql://user@host/db"),
            Target::Postgres("postgresql://user@host/db".into())
        );
        assert_eq!(
            Target::parse("sqlite://data/oxiledger.db"),
            Target::Sqlite("data/oxiledger.db".into())
        );
        assert_eq!(
            Target::parse("sqlite:///var/lib/oxiledger.db"),
            Target::Sqlite("/var/lib/oxiledger.db".into())
        );
        assert_eq!(
            Target::parse("ledger.db"),
            Target::Sqlite("ledger.db".into())
        );
        assert!(Target::parse("sqlite::memory:").is_in_memory());
    }
}
