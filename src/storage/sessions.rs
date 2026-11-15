use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::StorageResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcState {
    pub provider_id: String,
    pub csrf_token: String,
    pub pkce_verifier: String,
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn create(
        &self,
        user_id: Uuid,
        token: &str,
        expires_at: DateTime<Utc>,
    ) -> StorageResult<()>;

    async fn delete(&self, token: &str) -> StorageResult<()>;

    async fn find_active_user(&self, token: &str) -> StorageResult<Option<Uuid>>;

    async fn create_oidc_state(&self, state: OidcState) -> StorageResult<()>;

    async fn consume_oidc_state(&self, csrf_token: &str) -> StorageResult<Option<OidcState>>;
}
