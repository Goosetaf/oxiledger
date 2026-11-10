use async_trait::async_trait;
use diesel::prelude::*;

use super::schema::oidc_states;
use super::types::{Id, Timestamp};
use crate::storage::oidc_states::{OidcState, OidcStateRepository};
use crate::storage::StorageResult;

super::store!(OidcStateStore);

#[async_trait]
impl OidcStateRepository for OidcStateStore {
    async fn create(&self, state: OidcState) -> StorageResult<()> {
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

    async fn consume(&self, csrf_token: &str) -> StorageResult<Option<OidcState>> {
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
