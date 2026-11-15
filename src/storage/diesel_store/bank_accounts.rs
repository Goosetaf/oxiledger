use async_trait::async_trait;
use diesel::prelude::*;
use uuid::Uuid;

use super::connection::{write_transaction, DbConnection};
use super::schema::{accounts, bank_accounts, bank_auth_states, bank_connections};
use super::types::{Id, Json, Money, Timestamp};
use crate::models::bank_sync::{BankAccountRecord, BankAccountSummary};
use crate::storage::bank_accounts::{
    BankAccountDetails, BankAccountRepository, BankAuthState, BankConnectionDetails,
    NewBankConnection, ProviderAccount, ProviderSession, SyncCandidate, SyncableBankAccount,
};
use crate::storage::{StorageError, StorageResult};

super::store!(BankAccountStore);

diesel::define_sql_function! {
    #[sql_name = "coalesce"]
    fn coalesce_nullable(
        first: diesel::sql_types::Nullable<diesel::sql_types::Text>,
        second: diesel::sql_types::Nullable<diesel::sql_types::Text>,
    ) -> diesel::sql_types::Nullable<diesel::sql_types::Text>;
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = bank_accounts)]
struct BankAccountRow {
    id: Id,
    user_id: Id,
    bank_connection_id: Option<Id>,
    internal_account_id: Option<Id>,
    provider_account_uid: Option<String>,
    iban: Option<String>,
    name: Option<String>,
    currency: String,
    is_manual: bool,
    balance: Option<Money>,
    last_synced_at: Option<Timestamp>,
    created_at: Timestamp,
}

impl From<BankAccountRow> for BankAccountRecord {
    fn from(row: BankAccountRow) -> Self {
        BankAccountRecord {
            id: row.id.0,
            user_id: row.user_id.0,
            bank_connection_id: row.bank_connection_id.map(|id| id.0),
            internal_account_id: row.internal_account_id.map(|id| id.0),
            provider_account_uid: row.provider_account_uid,
            iban: row.iban,
            name: row.name,
            currency: row.currency,
            is_manual: row.is_manual,
            balance: row.balance.map(|money| money.0),
            last_synced_at: row.last_synced_at.map(|at| at.0),
            created_at: row.created_at.0,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = bank_accounts)]
struct NewBankAccountRow {
    id: Id,
    user_id: Id,
    bank_connection_id: Option<Id>,
    internal_account_id: Option<Id>,
    provider_account_uid: Option<String>,
    iban: Option<String>,
    name: Option<String>,
    currency: String,
    is_manual: bool,
    created_at: Timestamp,
}

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

type SummaryRow = (
    Id,
    Option<String>,
    Option<String>,
    String,
    Option<Id>,
    bool,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Timestamp>,
    Option<Timestamp>,
);

fn summary(row: SummaryRow) -> BankAccountSummary {
    let (
        id,
        name,
        iban,
        currency,
        internal_account_id,
        is_manual,
        internal_account_name,
        aspsp_name,
        aspsp_country,
        provider_id,
        access_valid_until,
        last_synced_at,
    ) = row;

    BankAccountSummary {
        id: id.0,
        name,
        iban,
        currency,
        internal_account_id: internal_account_id.map(|id| id.0),
        internal_account_name,
        aspsp_name,
        aspsp_country,
        provider_id,
        access_valid_until: access_valid_until.map(|at| at.0),
        last_synced_at: last_synced_at.map(|at| at.0),
        is_manual,
    }
}

fn load_summaries(
    conn: &mut DbConnection,
    user_id: Uuid,
    id: Option<Uuid>,
) -> QueryResult<Vec<BankAccountSummary>> {
    let bank_name = coalesce_nullable(bank_connections::aspsp_name, bank_accounts::name);

    let mut query = bank_accounts::table
        .left_join(bank_connections::table)
        .left_join(accounts::table)
        .filter(bank_accounts::user_id.eq(Id(user_id)))
        .select((
            bank_accounts::id,
            bank_accounts::name,
            bank_accounts::iban,
            bank_accounts::currency,
            bank_accounts::internal_account_id,
            bank_accounts::is_manual,
            accounts::name.nullable(),
            bank_connections::aspsp_name.nullable(),
            bank_connections::aspsp_country.nullable(),
            bank_connections::provider_id.nullable(),
            bank_connections::access_valid_until.nullable(),
            bank_accounts::last_synced_at,
        ))
        // NULL sorts first on SQLite and last on Postgres.
        .order((
            bank_name.is_null(),
            bank_name,
            bank_accounts::name.is_null(),
            bank_accounts::name,
        ))
        .into_boxed();

    if let Some(id) = id {
        query = query.filter(bank_accounts::id.eq(Id(id)));
    }

    Ok(query
        .load::<SummaryRow>(conn)?
        .into_iter()
        .map(summary)
        .collect())
}

fn save_provider_account(
    conn: &mut DbConnection,
    user_id: Uuid,
    connection_id: Uuid,
    account: &ProviderAccount,
    internal_account_id: Option<Uuid>,
) -> StorageResult<Uuid> {
    let existing = bank_accounts::table
        .filter(bank_accounts::bank_connection_id.eq(Id(connection_id)))
        .filter(bank_accounts::provider_account_uid.eq(&account.uid))
        .select(bank_accounts::id)
        .first::<Id>(conn)
        .optional()?;

    if let Some(id) = existing {
        diesel::update(bank_accounts::table.find(id))
            .set((
                bank_accounts::internal_account_id.eq(internal_account_id.map(Id)),
                bank_accounts::iban.eq(&account.iban),
                bank_accounts::name.eq(&account.name),
                bank_accounts::currency.eq(&account.currency),
                bank_accounts::is_manual.eq(false),
                bank_accounts::last_synced_at.eq(None::<Timestamp>),
                bank_accounts::balance.eq(None::<Money>),
            ))
            .execute(conn)?;
        return Ok(id.0);
    }

    let row = NewBankAccountRow {
        id: Id::new(),
        user_id: Id(user_id),
        bank_connection_id: Some(Id(connection_id)),
        internal_account_id: internal_account_id.map(Id),
        provider_account_uid: Some(account.uid.clone()),
        iban: account.iban.clone(),
        name: account.name.clone(),
        currency: account.currency.clone(),
        is_manual: false,
        created_at: Timestamp::now(),
    };

    diesel::insert_into(bank_accounts::table)
        .values(&row)
        .execute(conn)?;
    Ok(row.id.0)
}

#[async_trait]
impl BankAccountRepository for BankAccountStore {
    async fn list(&self, user_id: Uuid) -> StorageResult<Vec<BankAccountSummary>> {
        self.db
            .run(move |conn| Ok(load_summaries(conn, user_id, None)?))
            .await
    }

    async fn find_summary(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<BankAccountSummary>> {
        self.db
            .run(move |conn| Ok(load_summaries(conn, user_id, Some(id))?.pop()))
            .await
    }

    async fn find(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<BankAccountRecord>> {
        self.db
            .run(move |conn| {
                let row = bank_accounts::table
                    .filter(bank_accounts::id.eq(Id(id)))
                    .filter(bank_accounts::user_id.eq(Id(user_id)))
                    .select(BankAccountRow::as_select())
                    .first(conn)
                    .optional()?;
                Ok(row.map(BankAccountRecord::from))
            })
            .await
    }

    async fn list_for_connection(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
    ) -> StorageResult<Vec<BankAccountRecord>> {
        self.db
            .run(move |conn| {
                let rows = bank_accounts::table
                    .filter(bank_accounts::bank_connection_id.eq(Id(connection_id)))
                    .filter(bank_accounts::user_id.eq(Id(user_id)))
                    .select(BankAccountRow::as_select())
                    .load(conn)?;
                Ok(rows.into_iter().map(BankAccountRecord::from).collect())
            })
            .await
    }

    async fn count_for_connection(&self, connection_id: Uuid) -> StorageResult<i64> {
        self.db
            .run(move |conn| {
                Ok(bank_accounts::table
                    .filter(bank_accounts::bank_connection_id.eq(Id(connection_id)))
                    .count()
                    .get_result(conn)?)
            })
            .await
    }

    async fn create_manual(
        &self,
        user_id: Uuid,
        details: BankAccountDetails,
    ) -> StorageResult<Uuid> {
        self.db
            .run(move |conn| {
                let row = NewBankAccountRow {
                    id: Id::new(),
                    user_id: Id(user_id),
                    bank_connection_id: None,
                    internal_account_id: details.internal_account_id.map(Id),
                    provider_account_uid: None,
                    iban: details.iban,
                    name: Some(details.name),
                    currency: details.currency,
                    is_manual: true,
                    created_at: Timestamp::now(),
                };

                diesel::insert_into(bank_accounts::table)
                    .values(&row)
                    .execute(conn)?;
                Ok(row.id.0)
            })
            .await
    }

    async fn update(
        &self,
        user_id: Uuid,
        id: Uuid,
        details: BankAccountDetails,
    ) -> StorageResult<bool> {
        self.db
            .run(move |conn| {
                let updated = diesel::update(
                    bank_accounts::table
                        .filter(bank_accounts::id.eq(Id(id)))
                        .filter(bank_accounts::user_id.eq(Id(user_id))),
                )
                .set((
                    bank_accounts::name.eq(details.name),
                    bank_accounts::iban.eq(details.iban),
                    bank_accounts::currency.eq(details.currency),
                    bank_accounts::internal_account_id.eq(details.internal_account_id.map(Id)),
                ))
                .execute(conn)?;
                Ok(updated > 0)
            })
            .await
    }

    async fn delete(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
        self.db
            .run(move |conn| {
                let deleted = diesel::delete(
                    bank_accounts::table
                        .filter(bank_accounts::id.eq(Id(id)))
                        .filter(bank_accounts::user_id.eq(Id(user_id))),
                )
                .execute(conn)?;
                Ok(deleted > 0)
            })
            .await
    }

    async fn map_provider_account(
        &self,
        user_id: Uuid,
        connection_id: Uuid,
        account: ProviderAccount,
        internal_account_id: Option<Uuid>,
    ) -> StorageResult<Uuid> {
        self.db
            .run(move |conn| {
                let mut attempt = || {
                    write_transaction(conn, |conn| {
                        save_provider_account(
                            conn,
                            user_id,
                            connection_id,
                            &account,
                            internal_account_id,
                        )
                    })
                };

                match attempt() {
                    // A concurrent request added the account first, so the retry updates it.
                    Err(StorageError::UniqueViolation(_)) => attempt(),
                    result => result,
                }
            })
            .await
    }

    async fn connect_to_provider(
        &self,
        user_id: Uuid,
        id: Uuid,
        connection_id: Uuid,
        account: ProviderAccount,
    ) -> StorageResult<bool> {
        #[derive(AsChangeset)]
        #[diesel(table_name = bank_accounts)]
        struct ProvidedDetails {
            iban: Option<String>,
            name: Option<String>,
        }

        self.db
            .run(move |conn| {
                let updated = diesel::update(
                    bank_accounts::table
                        .filter(bank_accounts::id.eq(Id(id)))
                        .filter(bank_accounts::user_id.eq(Id(user_id))),
                )
                .set((
                    ProvidedDetails {
                        iban: account.iban,
                        name: account.name,
                    },
                    bank_accounts::bank_connection_id.eq(Some(Id(connection_id))),
                    bank_accounts::provider_account_uid.eq(Some(account.uid)),
                    bank_accounts::currency.eq(account.currency),
                    bank_accounts::is_manual.eq(false),
                    bank_accounts::last_synced_at.eq(None::<Timestamp>),
                    bank_accounts::balance.eq(None::<Money>),
                ))
                .execute(conn)?;
                Ok(updated > 0)
            })
            .await
    }

    async fn disconnect_from_provider(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
        self.db
            .run(move |conn| {
                let updated = diesel::update(
                    bank_accounts::table
                        .filter(bank_accounts::id.eq(Id(id)))
                        .filter(bank_accounts::user_id.eq(Id(user_id))),
                )
                .set((
                    bank_accounts::bank_connection_id.eq(None::<Id>),
                    bank_accounts::provider_account_uid.eq(None::<String>),
                    bank_accounts::is_manual.eq(true),
                    bank_accounts::last_synced_at.eq(None::<Timestamp>),
                    bank_accounts::balance.eq(None::<Money>),
                ))
                .execute(conn)?;
                Ok(updated > 0)
            })
            .await
    }

    async fn find_syncable(
        &self,
        id: Uuid,
        user_id: Option<Uuid>,
    ) -> StorageResult<Option<SyncableBankAccount>> {
        type Row = (
            Id,
            Id,
            bool,
            Option<String>,
            Option<Timestamp>,
            Option<Id>,
            Option<String>,
            Option<String>,
        );

        self.db
            .run(move |conn| {
                let mut query = bank_accounts::table
                    .left_join(bank_connections::table)
                    .filter(bank_accounts::id.eq(Id(id)))
                    .select((
                        bank_accounts::id,
                        bank_accounts::user_id,
                        bank_accounts::is_manual,
                        bank_accounts::provider_account_uid,
                        bank_accounts::last_synced_at,
                        bank_connections::id.nullable(),
                        bank_connections::provider_id.nullable(),
                        bank_connections::provider_session_id.nullable(),
                    ))
                    .into_boxed();

                if let Some(user_id) = user_id {
                    query = query.filter(bank_accounts::user_id.eq(Id(user_id)));
                }

                let row = query.first::<Row>(conn).optional()?;

                Ok(row.map(
                    |(
                        id,
                        user_id,
                        is_manual,
                        provider_account_uid,
                        last_synced_at,
                        connection_id,
                        provider_id,
                        provider_session_id,
                    )| SyncableBankAccount {
                        id: id.0,
                        user_id: user_id.0,
                        is_manual,
                        provider_account_uid,
                        last_synced_at: last_synced_at.map(|at| at.0),
                        session: match (connection_id, provider_id, provider_session_id) {
                            (Some(connection_id), Some(provider_id), Some(provider_session_id)) => {
                                Some(ProviderSession {
                                    connection_id: connection_id.0,
                                    provider_id,
                                    provider_session_id,
                                })
                            }
                            _ => None,
                        },
                    },
                ))
            })
            .await
    }

    async fn list_sync_candidates(&self) -> StorageResult<Vec<SyncCandidate>> {
        self.db
            .run(move |conn| {
                let rows = bank_accounts::table
                    .filter(bank_accounts::is_manual.eq(false))
                    .filter(bank_accounts::bank_connection_id.is_not_null())
                    .filter(bank_accounts::provider_account_uid.is_not_null())
                    .select((bank_accounts::id, bank_accounts::last_synced_at))
                    .load::<(Id, Option<Timestamp>)>(conn)?;

                Ok(rows
                    .into_iter()
                    .map(|(id, last_synced_at)| SyncCandidate {
                        id: id.0,
                        last_synced_at: last_synced_at.map(|at| at.0),
                    })
                    .collect())
            })
            .await
    }

    async fn linked_provider_uids(
        &self,
        user_id: Uuid,
        uids: Vec<String>,
        outside_connection_id: Uuid,
    ) -> StorageResult<Vec<String>> {
        self.db
            .run(move |conn| {
                let linked = bank_accounts::table
                    .filter(bank_accounts::user_id.eq(Id(user_id)))
                    .filter(bank_accounts::provider_account_uid.eq_any(uids))
                    .filter(
                        bank_accounts::bank_connection_id
                            .is_null()
                            .or(bank_accounts::bank_connection_id.ne(Id(outside_connection_id))),
                    )
                    .select(bank_accounts::provider_account_uid)
                    .distinct()
                    .load::<Option<String>>(conn)?;

                Ok(linked.into_iter().flatten().collect())
            })
            .await
    }

    async fn create_connection(&self, connection: NewBankConnection) -> StorageResult<Uuid> {
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

    async fn find_connection(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<BankConnectionDetails>> {
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

    async fn delete_connection(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
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

    async fn create_auth_state(&self, state: BankAuthState) -> StorageResult<()> {
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

    async fn consume_auth_state(
        &self,
        user_id: Uuid,
        state: &str,
    ) -> StorageResult<Option<BankAuthState>> {
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
