use async_trait::async_trait;
use diesel::prelude::*;
use uuid::Uuid;

use super::schema::bank_connections;
use super::types::{Id, Json, Timestamp};
use crate::storage::bank_connections::{
    BankConnectionDetails, BankConnectionRepository, NewBankConnection,
};
use crate::storage::{StorageError, StorageResult};

super::store!(BankConnectionStore);

#[async_trait]
impl BankConnectionRepository for BankConnectionStore {
    async fn create(&self, connection: NewBankConnection) -> StorageResult<Uuid> {
        let provider_accounts = serde_json::to_value(&connection.provider_accounts)
            .map_err(|err| StorageError::Other(err.to_string()))?;

        self.db
            .run(move |conn| {
                let id = Uuid::new_v4();
                diesel::insert_into(bank_connections::table)
                    .values((
                        bank_connections::id.eq(Id(id)),
                        bank_connections::user_id.eq(Id(connection.user_id)),
                        bank_connections::provider_id.eq(connection.provider_id),
                        bank_connections::provider_session_id.eq(connection.provider_session_id),
                        bank_connections::aspsp_name.eq(connection.aspsp_name),
                        bank_connections::aspsp_country.eq(connection.aspsp_country),
                        bank_connections::access_valid_until
                            .eq(Some(Timestamp(connection.access_valid_until))),
                        bank_connections::provider_accounts_json.eq(Some(Json(provider_accounts))),
                        bank_connections::source_bank_account_id
                            .eq(connection.source_bank_account_id.map(Id)),
                        bank_connections::created_at.eq(Timestamp::now()),
                    ))
                    .execute(conn)?;
                Ok(id)
            })
            .await
    }

    async fn find(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<BankConnectionDetails>> {
        self.db
            .run(move |conn| {
                let row = bank_connections::table
                    .filter(bank_connections::id.eq(Id(id)))
                    .filter(bank_connections::user_id.eq(Id(user_id)))
                    .select((
                        bank_connections::source_bank_account_id,
                        bank_connections::provider_accounts_json,
                    ))
                    .first::<(Option<Id>, Option<Json>)>(conn)
                    .optional()?;

                let Some((source_bank_account_id, provider_accounts)) = row else {
                    return Ok(None);
                };

                let provider_accounts = provider_accounts
                    .map(|json| serde_json::from_value(json.0))
                    .transpose()
                    .map_err(|err| StorageError::Other(err.to_string()))?
                    .unwrap_or_default();

                Ok(Some(BankConnectionDetails {
                    source_bank_account_id: source_bank_account_id.map(|id| id.0),
                    provider_accounts,
                }))
            })
            .await
    }

    async fn delete(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
        self.db
            .run(move |conn| {
                let deleted = diesel::delete(
                    bank_connections::table
                        .filter(bank_connections::id.eq(Id(id)))
                        .filter(bank_connections::user_id.eq(Id(user_id))),
                )
                .execute(conn)?;
                Ok(deleted > 0)
            })
            .await
    }
}
