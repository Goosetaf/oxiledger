use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use uuid::Uuid;

use super::schema::sessions;
use super::types::{Id, Timestamp};
use crate::storage::sessions::SessionRepository;
use crate::storage::StorageResult;

super::store!(SessionStore);

#[async_trait]
impl SessionRepository for SessionStore {
    async fn create(
        &self,
        user_id: Uuid,
        token: &str,
        expires_at: DateTime<Utc>,
    ) -> StorageResult<()> {
        let token = token.to_string();
        self.db
            .run(move |conn| {
                diesel::insert_into(sessions::table)
                    .values((
                        sessions::id.eq(Id::new()),
                        sessions::user_id.eq(Id(user_id)),
                        sessions::token.eq(token),
                        sessions::expires_at.eq(Timestamp(expires_at)),
                        sessions::created_at.eq(Timestamp::now()),
                    ))
                    .execute(conn)?;
                Ok(())
            })
            .await
    }

    async fn delete(&self, token: &str) -> StorageResult<()> {
        let token = token.to_string();
        self.db
            .run(move |conn| {
                diesel::delete(sessions::table.filter(sessions::token.eq(token))).execute(conn)?;
                Ok(())
            })
            .await
    }

    async fn find_active_user(&self, token: &str) -> StorageResult<Option<Uuid>> {
        let token = token.to_string();
        self.db
            .run(move |conn| {
                let user_id = sessions::table
                    .filter(sessions::token.eq(token))
                    .filter(sessions::expires_at.gt(Timestamp::now()))
                    .select(sessions::user_id)
                    .first::<Id>(conn)
                    .optional()?;
                Ok(user_id.map(|id| id.0))
            })
            .await
    }
}
