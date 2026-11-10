use async_trait::async_trait;
use chrono::{DateTime, Utc};

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
pub trait OidcStateRepository: Send + Sync {
    async fn create(&self, state: OidcState) -> StorageResult<()>;

    /// Removes and returns the unexpired state for a CSRF token, so it can be used only once.
    async fn consume(&self, csrf_token: &str) -> StorageResult<Option<OidcState>>;
}
