use async_trait::async_trait;
use diesel::prelude::*;
use uuid::Uuid;

use super::schema::bank_auth_states;
use super::types::{Id, Timestamp};
use crate::storage::bank_auth_states::{BankAuthState, BankAuthStateRepository};
use crate::storage::StorageResult;

super::store!(BankAuthStateStore);

type BankAuthStateRow = (
    Id,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Id>,
    Timestamp,
);

#[async_trait]
impl BankAuthStateRepository for BankAuthStateStore {
    async fn create(&self, state: BankAuthState) -> StorageResult<()> {
        self.db
            .run(move |conn| {
                diesel::insert_into(bank_auth_states::table)
                    .values((
                        bank_auth_states::id.eq(Id::new()),
                        bank_auth_states::user_id.eq(Id(state.user_id)),
                        bank_auth_states::provider_id.eq(state.provider_id),
                        bank_auth_states::state.eq(state.state),
                        bank_auth_states::aspsp_name.eq(state.aspsp_name),
                        bank_auth_states::aspsp_country.eq(state.aspsp_country),
                        bank_auth_states::provider_reference.eq(state.provider_reference),
                        bank_auth_states::source_bank_account_id
                            .eq(state.source_bank_account_id.map(Id)),
                        bank_auth_states::expires_at.eq(Timestamp(state.expires_at)),
                        bank_auth_states::created_at.eq(Timestamp::now()),
                    ))
                    .execute(conn)?;
                Ok(())
            })
            .await
    }

    async fn consume(&self, user_id: Uuid, state: &str) -> StorageResult<Option<BankAuthState>> {
        let state = state.to_string();
        self.db
            .run(move |conn| {
                let row = diesel::delete(
                    bank_auth_states::table
                        .filter(bank_auth_states::state.eq(state))
                        .filter(bank_auth_states::user_id.eq(Id(user_id)))
                        .filter(bank_auth_states::expires_at.gt(Timestamp::now())),
                )
                .returning((
                    bank_auth_states::user_id,
                    bank_auth_states::provider_id,
                    bank_auth_states::state,
                    bank_auth_states::aspsp_name,
                    bank_auth_states::aspsp_country,
                    bank_auth_states::provider_reference,
                    bank_auth_states::source_bank_account_id,
                    bank_auth_states::expires_at,
                ))
                .get_result::<BankAuthStateRow>(conn)
                .optional()?;

                Ok(row.map(
                    |(
                        user_id,
                        provider_id,
                        state,
                        aspsp_name,
                        aspsp_country,
                        provider_reference,
                        source_bank_account_id,
                        expires_at,
                    )| BankAuthState {
                        user_id: user_id.0,
                        provider_id,
                        state,
                        aspsp_name,
                        aspsp_country,
                        provider_reference,
                        source_bank_account_id: source_bank_account_id.map(|id| id.0),
                        expires_at: expires_at.0,
                    },
                ))
            })
            .await
    }
}
