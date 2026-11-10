//! Throwaway databases for the repository tests.

use std::path::PathBuf;

use diesel::connection::SimpleConnection;
use diesel::{Connection, PgConnection};
use uuid::Uuid;

use crate::storage::Storage;

/// Set to a Postgres URL to run the repository tests against Postgres as well as SQLite.
pub const POSTGRES_URL_VAR: &str = "OXILEDGER_TEST_POSTGRES_URL";

pub struct TestDatabase {
    pub storage: Storage,
    _cleanup: Cleanup,
}

pub enum Cleanup {
    SqliteFile(PathBuf),
    PostgresSchema { url: String, schema: String },
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        match self {
            Cleanup::SqliteFile(path) => {
                for suffix in ["", "-wal", "-shm"] {
                    let mut file = path.clone().into_os_string();
                    file.push(suffix);
                    let _ = std::fs::remove_file(file);
                }
            }
            Cleanup::PostgresSchema { url, schema } => {
                if let Ok(mut conn) = PgConnection::establish(url) {
                    let _ = conn.batch_execute(&format!("DROP SCHEMA \"{schema}\" CASCADE"));
                }
            }
        }
    }
}

/// A file rather than an in-memory database, so the tests use several connections like the
/// running application does.
pub async fn sqlite() -> TestDatabase {
    let path = std::env::temp_dir().join(format!("oxiledger-test-{}.db", Uuid::new_v4()));
    let storage = Storage::connect(&format!("sqlite://{}", path.display()))
        .await
        .expect("failed to open the SQLite test database");

    TestDatabase {
        storage,
        _cleanup: Cleanup::SqliteFile(path),
    }
}

/// A schema of its own in the database named by `OXILEDGER_TEST_POSTGRES_URL`, or `None` when
/// the variable is not set.
pub async fn postgres() -> Option<TestDatabase> {
    let (url, cleanup) = postgres_schema()?;
    let storage = Storage::connect(&url)
        .await
        .expect("failed to open the Postgres test database");

    Some(TestDatabase {
        storage,
        _cleanup: cleanup,
    })
}

/// Creates an empty schema and returns a URL that connects into it.
pub fn postgres_schema() -> Option<(String, Cleanup)> {
    let url = std::env::var(POSTGRES_URL_VAR).ok()?;
    let schema = format!("test_{}", Uuid::new_v4().simple());

    let mut conn = PgConnection::establish(&url).expect("failed to connect to test Postgres");
    conn.batch_execute(&format!("CREATE SCHEMA \"{schema}\""))
        .expect("failed to create the test schema");

    let separator = if url.contains('?') { '&' } else { '?' };
    let scoped_url = format!("{url}{separator}options=-c%20search_path%3D{schema}");

    Some((scoped_url, Cleanup::PostgresSchema { url, schema }))
}
