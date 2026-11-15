use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use uuid::Uuid;

use super::schema::{oidc_states, sessions};
use super::types::{Id, Timestamp};
use crate::storage::sessions::{OidcState, SessionRepository};
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

    async fn create_oidc_state(&self, state: OidcState) -> StorageResult<()> {
        self.db
            .run(move |conn| {
                diesel::insert_into(oidc_states::table)
                    .values((
                        oidc_states::id.eq(Id::new()),
                        oidc_states::provider_id.eq(state.provider_id),
                        oidc_states::csrf_token.eq(state.csrf_token),
                        oidc_states::pkce_verifier.eq(state.pkce_verifier),
                        oidc_states::nonce.eq(state.nonce),
                        oidc_states::expires_at.eq(Timestamp(state.expires_at)),
                        oidc_states::created_at.eq(Timestamp::now()),
                    ))
                    .execute(conn)?;
                Ok(())
            })
            .await
    }

    async fn consume_oidc_state(&self, csrf_token: &str) -> StorageResult<Option<OidcState>> {
        let csrf_token = csrf_token.to_string();
        self.db
            .run(move |conn| {
                let row = diesel::delete(
                    oidc_states::table
                        .filter(oidc_states::csrf_token.eq(csrf_token))
                        .filter(oidc_states::expires_at.gt(Timestamp::now())),
                )
                .returning((
                    oidc_states::provider_id,
                    oidc_states::csrf_token,
                    oidc_states::pkce_verifier,
                    oidc_states::nonce,
                    oidc_states::expires_at,
                ))
                .get_result::<(String, String, String, String, Timestamp)>(conn)
                .optional()?;

                Ok(row.map(
                    |(provider_id, csrf_token, pkce_verifier, nonce, expires_at)| OidcState {
                        provider_id,
                        csrf_token,
                        pkce_verifier,
                        nonce,
                        expires_at: expires_at.0,
                    },
                ))
            })
            .await
    }
}
