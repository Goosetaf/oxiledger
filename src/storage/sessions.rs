use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::StorageResult;

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn create(
        &self,
        user_id: Uuid,
        token: &str,
        expires_at: DateTime<Utc>,
    ) -> StorageResult<()>;

    async fn delete(&self, token: &str) -> StorageResult<()>;

    /// The user behind a session that has not expired.
    async fn find_active_user(&self, token: &str) -> StorageResult<Option<Uuid>>;
}
