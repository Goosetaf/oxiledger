use diesel::connection::SimpleConnection;
use diesel::PgConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

use super::connection::DbConnection;
use crate::storage::{StorageError, StorageResult};

const POSTGRES: EmbeddedMigrations = embed_migrations!("migrations/postgres");
const SQLITE: EmbeddedMigrations = embed_migrations!("migrations/sqlite");

const MIGRATION_LOCK: i64 = 0x6f78_696c_6564_6765;

pub fn run(conn: &mut DbConnection) -> StorageResult<()> {
    match conn {
        DbConnection::Postgres(pg) => {
            // Keeps instances that start together from migrating at the same time.
            pg.batch_execute(&format!("SELECT pg_advisory_lock({MIGRATION_LOCK})"))?;
            let migrated = adopt_sqlx_history(pg).and_then(|()| {
                pg.run_pending_migrations(POSTGRES)
                    .map(drop)
                    .map_err(failed)
            });
            pg.batch_execute(&format!("SELECT pg_advisory_unlock({MIGRATION_LOCK})"))?;
            migrated
        }
        // Holds the write lock from before the pending migrations are worked out, so an
        // instance starting at the same time waits and then finds nothing left to run.
        DbConnection::Sqlite(sqlite) => sqlite.immediate_transaction(|sqlite| {
            sqlite
                .run_pending_migrations(SQLITE)
                .map(drop)
                .map_err(failed)
        }),
    }
}

fn failed(err: Box<dyn std::error::Error + Send + Sync>) -> StorageError {
    StorageError::Other(format!("Failed to run database migrations: {err}"))
}

/// Databases created before the move to Diesel recorded their migrations in sqlx's table.
/// Copies that record over, so those migrations are not run a second time.
fn adopt_sqlx_history(pg: &mut PgConnection) -> StorageResult<()> {
    pg.batch_execute(
        r#"
        DO $$
        BEGIN
            IF to_regclass('_sqlx_migrations') IS NOT NULL THEN
                CREATE TABLE IF NOT EXISTS __diesel_schema_migrations (
                    version VARCHAR(50) PRIMARY KEY NOT NULL,
                    run_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                INSERT INTO __diesel_schema_migrations (version)
                SELECT lpad(version::text, 4, '0') FROM _sqlx_migrations WHERE success
                ON CONFLICT (version) DO NOTHING;
            END IF;
        END
        $$;
        "#,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use diesel::connection::SimpleConnection;
    use diesel::{Connection, PgConnection};

    use crate::storage::diesel_store::test_support;
    use crate::storage::Storage;

    #[tokio::test]
    async fn instances_starting_together_on_a_new_sqlite_file_all_start() {
        let path = std::env::temp_dir().join(format!("oxiledger-test-{}.db", uuid::Uuid::new_v4()));
        let url = format!("sqlite://{}", path.display());
        let _cleanup = test_support::Cleanup::SqliteFile(path);

        let starts = (0..6).map(|_| {
            let url = url.clone();
            tokio::spawn(async move { Storage::connect(&url).await })
        });

        for start in starts.collect::<Vec<_>>() {
            start.await.unwrap().unwrap();
        }
    }

    #[tokio::test]
    async fn a_database_migrated_by_sqlx_keeps_its_data_and_is_not_migrated_again() {
        let Some((url, _cleanup)) = test_support::postgres_schema() else {
            return;
        };

        let mut conn = PgConnection::establish(&url).unwrap();
        for migration in [
            include_str!("../../../migrations/postgres/0001_initial/up.sql"),
            include_str!("../../../migrations/postgres/0002_oidc/up.sql"),
            include_str!("../../../migrations/postgres/0003_bank_sync/up.sql"),
            include_str!("../../../migrations/postgres/0004_user_preferences/up.sql"),
        ] {
            conn.batch_execute(migration).unwrap();
        }
        conn.batch_execute(
            "CREATE TABLE _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
                success BOOLEAN NOT NULL,
                checksum BYTEA NOT NULL,
                execution_time BIGINT NOT NULL
            );
            INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
            VALUES (1, 'initial', TRUE, '', 0), (2, 'oidc', TRUE, '', 0),
                   (3, 'bank sync', TRUE, '', 0), (4, 'user preferences', TRUE, '', 0);
            INSERT INTO users (username, email, password_hash)
            VALUES ('ada', 'ada@example.com', 'hash');",
        )
        .unwrap();
        drop(conn);

        let storage = Storage::connect(&url).await.unwrap();
        // Starting again finds nothing left to do.
        Storage::connect(&url).await.unwrap();

        let credentials = storage
            .users
            .find_credentials("ada")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(credentials.password_hash.as_deref(), Some("hash"));
        let current = storage
            .users
            .find_current(credentials.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            current.preferences,
            crate::preferences::Preferences::default()
        );
    }
}
