use sqlx::PgPool;

/// Initialize a PostgreSQL connection pool from the DATABASE_URL environment variable.
pub async fn connect() -> PgPool {
    dotenvy::dotenv().ok();
    let database_url =
        std::env::var("DATABASE_URL").expect("DATABASE_URL environment variable must be set");
    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to PostgreSQL")
}

/// Run any pending sqlx migrations from the ./migrations directory.
pub async fn run_migrations(pool: &PgPool) {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run database migrations");
}
