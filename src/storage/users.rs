use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::StorageResult;
use crate::{models::user::CurrentUser, preferences::Preferences};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcIdentity {
    pub provider: String,
    pub subject: String,
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
    pub oidc: Option<OidcIdentity>,
}

#[derive(Debug, Clone)]
pub struct UserCredentials {
    pub id: Uuid,
    pub password_hash: Option<String>,
}

/// The PKCE and CSRF values kept while a user is away signing in with an OIDC provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcState {
    pub provider_id: String,
    pub csrf_token: String,
    pub pkce_verifier: String,
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
}

/// Users and how they sign in: accounts, sessions, and OIDC sign-ins in progress.
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn any_exist(&self) -> StorageResult<bool>;

    async fn find_current(&self, id: Uuid) -> StorageResult<Option<CurrentUser>>;

    async fn find_credentials(&self, username: &str) -> StorageResult<Option<UserCredentials>>;

    async fn is_taken(&self, username: &str, email: &str) -> StorageResult<bool>;

    async fn create(&self, user: NewUser) -> StorageResult<Uuid>;

    /// Creates the user only if no user exists yet. Concurrent callers cannot both succeed.
    async fn create_first(&self, user: NewUser) -> StorageResult<Option<Uuid>>;

    async fn has_oidc_identity(&self, identity: &OidcIdentity) -> StorageResult<bool>;

    /// Creates the user behind an identity, or refreshes the email of the one that has it.
    async fn upsert_oidc(
        &self,
        username: &str,
        email: &str,
        identity: &OidcIdentity,
    ) -> StorageResult<Uuid>;

    async fn save_preferences(&self, id: Uuid, preferences: &Preferences) -> StorageResult<()>;

    async fn create_session(
        &self,
        user_id: Uuid,
        token: &str,
        expires_at: DateTime<Utc>,
    ) -> StorageResult<()>;

    async fn delete_session(&self, token: &str) -> StorageResult<()>;

    /// The user behind a session that has not expired.
    async fn find_session_user(&self, token: &str) -> StorageResult<Option<Uuid>>;

    async fn create_oidc_state(&self, state: OidcState) -> StorageResult<()>;

    /// Removes and returns the unexpired state for a CSRF token, so it can be used only once.
    async fn consume_oidc_state(&self, csrf_token: &str) -> StorageResult<Option<OidcState>>;
}
