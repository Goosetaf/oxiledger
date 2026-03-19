use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
// Note: we intentionally avoid using axum extractors here; read extensions
// directly from `FullstackContext` to avoid axum_core version conflicts.
use dioxus::prelude::{dioxus_fullstack::FullstackContext, HttpError, ServerFnError, StatusCode};
use sqlx::PgPool;
use tower_cookies::Cookies;
use uuid::Uuid;

pub const SESSION_COOKIE: &str = "session_token";
/// Sessions expire after 30 days.
const SESSION_DURATION_DAYS: i64 = 30;

/// Hash a plaintext password using Argon2id.
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    Ok(argon2
        .hash_password(password.as_bytes(), &salt)?
        .to_string())
}

/// Verify a plaintext password against a stored Argon2 hash.
pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Create a new session for the given user and return the session token.
pub async fn create_session(pool: &PgPool, user_id: Uuid) -> Result<String, sqlx::Error> {
    let token = Uuid::new_v4().to_string();
    let expires_at = chrono::Utc::now() + chrono::Duration::days(SESSION_DURATION_DAYS);

    sqlx::query!(
        "INSERT INTO sessions (user_id, token, expires_at) VALUES ($1, $2, $3)",
        user_id,
        token,
        expires_at
    )
    .execute(pool)
    .await?;

    Ok(token)
}

/// Delete a session by its token (logout).
pub async fn delete_session(pool: &PgPool, token: &str) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM sessions WHERE token = $1", token)
        .execute(pool)
        .await?;
    Ok(())
}

/// Extract and validate the session cookie, returning the authenticated user_id.
/// Returns `Err` with a suitable message if unauthenticated or the session has expired.
pub async fn require_auth(pool: &PgPool, cookies: &Cookies) -> Result<Uuid, ServerFnError> {
    let token = cookies
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| {
            ServerFnError::from(HttpError::new(
                StatusCode::UNAUTHORIZED,
                "Not authenticated",
            ))
        })?;

    let row = sqlx::query!(
        "SELECT user_id FROM sessions WHERE token = $1 AND expires_at > NOW()",
        token
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?
    .ok_or_else(|| {
        ServerFnError::from(HttpError::new(
            StatusCode::UNAUTHORIZED,
            "Session expired or invalid",
        ))
    })?;

    Ok(row.user_id)
}

/// Extract PgPool and session cookie value from the request context inside a server function.
/// Returns `(pool, Option<session_token>)` where the session token is taken from the
/// `Cookie` header if present.
pub async fn extract_context() -> Result<(PgPool, Cookies), ServerFnError> {
    // Prefer directly reading values from the FullstackContext to avoid
    // dealing with differing axum_core generic parameters across
    // dependency versions. This reads the request extensions that were
    // previously injected by middleware (eg. `CookieManagerLayer` and
    // `Extension(pool)` in `main.rs`).
    let ctx = FullstackContext::current()
        .ok_or_else(|| ServerFnError::new("No FullstackContext available"))?;

    let pool = ctx
        .extension::<PgPool>()
        .ok_or_else(|| ServerFnError::new("Missing database pool extension"))?;

    let cookies = ctx.extension::<Cookies>().ok_or_else(|| {
        ServerFnError::new("Missing Cookies extension. Is CookieManagerLayer enabled?")
    })?;

    Ok((pool, cookies))
}
