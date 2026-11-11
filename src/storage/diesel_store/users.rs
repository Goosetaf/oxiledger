use async_trait::async_trait;
use chrono::{DateTime, Utc};
use diesel::dsl::exists;
use diesel::prelude::*;
use diesel::upsert::excluded;
use uuid::Uuid;

use super::connection::{on_backend, write_transaction, DbConnection};
use super::schema::{oidc_states, sessions, users};
use super::types::{Id, Json, Timestamp};
use crate::models::user::CurrentUser;
use crate::preferences::Preferences;
use crate::storage::users::{NewUser, OidcIdentity, OidcState, UserCredentials, UserRepository};
use crate::storage::{StorageError, StorageResult};

super::store!(UserStore);

#[derive(Insertable)]
#[diesel(table_name = users)]
struct NewUserRow {
    id: Id,
    username: String,
    email: String,
    password_hash: Option<String>,
    oidc_provider: Option<String>,
    oidc_subject: Option<String>,
    preferences: Json,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl From<NewUser> for NewUserRow {
    fn from(user: NewUser) -> Self {
        let now = Timestamp::now();
        let (oidc_provider, oidc_subject) = match user.oidc {
            Some(identity) => (Some(identity.provider), Some(identity.subject)),
            None => (None, None),
        };

        Self {
            id: Id::new(),
            username: user.username,
            email: user.email,
            password_hash: user.password_hash,
            oidc_provider,
            oidc_subject,
            preferences: Json(serde_json::json!({})),
            created_at: now,
            updated_at: now,
        }
    }
}

fn any_users(conn: &mut DbConnection) -> QueryResult<bool> {
    diesel::select(exists(users::table.select(users::id))).get_result(conn)
}

#[async_trait]
impl UserRepository for UserStore {
    async fn any_exist(&self) -> StorageResult<bool> {
        self.db.run(|conn| Ok(any_users(conn)?)).await
    }

    async fn find_current(&self, id: Uuid) -> StorageResult<Option<CurrentUser>> {
        self.db
            .run(move |conn| {
                let row = users::table
                    .find(Id(id))
                    .select((users::username, users::preferences))
                    .first::<(String, Json)>(conn)
                    .optional()?;

                Ok(row.map(|(username, preferences)| CurrentUser {
                    username,
                    preferences: serde_json::from_value(preferences.0).unwrap_or_default(),
                }))
            })
            .await
    }

    async fn find_credentials(&self, username: &str) -> StorageResult<Option<UserCredentials>> {
        let username = username.to_string();
        self.db
            .run(move |conn| {
                let row = users::table
                    .filter(users::username.eq(username))
                    .select((users::id, users::password_hash))
                    .first::<(Id, Option<String>)>(conn)
                    .optional()?;

                Ok(row.map(|(id, password_hash)| UserCredentials {
                    id: id.0,
                    password_hash,
                }))
            })
            .await
    }

    async fn is_taken(&self, username: &str, email: &str) -> StorageResult<bool> {
        let username = username.to_string();
        let email = email.to_string();
        self.db
            .run(move |conn| {
                Ok(diesel::select(exists(
                    users::table
                        .filter(users::username.eq(username).or(users::email.eq(email)))
                        .select(users::id),
                ))
                .get_result(conn)?)
            })
            .await
    }

    async fn create(&self, user: NewUser) -> StorageResult<Uuid> {
        let row = NewUserRow::from(user);
        self.db
            .run(move |conn| {
                diesel::insert_into(users::table)
                    .values(&row)
                    .execute(conn)?;
                Ok(row.id.0)
            })
            .await
    }

    async fn create_first(&self, user: NewUser) -> StorageResult<Option<Uuid>> {
        let row = NewUserRow::from(user);
        self.db
            .run(move |conn| {
                // Skips the lock once set up; the check under the lock is what makes this safe.
                if any_users(conn)? {
                    return Ok(None);
                }

                write_transaction(conn, |conn| {
                    // Makes concurrent callers wait their turn. On SQLite, the write
                    // transaction itself does that.
                    if let DbConnection::Postgres(pg) = conn {
                        diesel::sql_query("LOCK TABLE users IN SHARE ROW EXCLUSIVE MODE")
                            .execute(pg)?;
                    }

                    if any_users(conn)? {
                        return Ok(None);
                    }

                    diesel::insert_into(users::table)
                        .values(&row)
                        .execute(conn)?;
                    Ok(Some(row.id.0))
                })
            })
            .await
    }

    async fn has_oidc_identity(&self, identity: &OidcIdentity) -> StorageResult<bool> {
        let identity = identity.clone();
        self.db
            .run(move |conn| {
                Ok(diesel::select(exists(
                    users::table
                        .filter(users::oidc_provider.eq(identity.provider))
                        .filter(users::oidc_subject.eq(identity.subject))
                        .select(users::id),
                ))
                .get_result(conn)?)
            })
            .await
    }

    async fn upsert_oidc(
        &self,
        username: &str,
        email: &str,
        identity: &OidcIdentity,
    ) -> StorageResult<Uuid> {
        let row = NewUserRow::from(NewUser {
            username: username.to_string(),
            email: email.to_string(),
            password_hash: None,
            oidc: Some(identity.clone()),
        });

        self.db
            .run(move |conn| {
                let id = on_backend!(conn, |c| diesel::insert_into(users::table)
                    .values(&row)
                    .on_conflict((users::oidc_provider, users::oidc_subject))
                    .do_update()
                    .set((
                        users::email.eq(excluded(users::email)),
                        users::updated_at.eq(excluded(users::updated_at)),
                    ))
                    .returning(users::id)
                    .get_result::<Id>(c))?;
                Ok(id.0)
            })
            .await
    }

    async fn save_preferences(&self, id: Uuid, preferences: &Preferences) -> StorageResult<()> {
        let stored = serde_json::to_value(preferences)
            .map_err(|err| StorageError::Other(err.to_string()))?;
        self.db
            .run(move |conn| {
                diesel::update(users::table.find(Id(id)))
                    .set(users::preferences.eq(Json(stored)))
                    .execute(conn)?;
                Ok(())
            })
            .await
    }

    async fn create_session(
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

    async fn delete_session(&self, token: &str) -> StorageResult<()> {
        let token = token.to_string();
        self.db
            .run(move |conn| {
                diesel::delete(sessions::table.filter(sessions::token.eq(token))).execute(conn)?;
                Ok(())
            })
            .await
    }

    async fn find_session_user(&self, token: &str) -> StorageResult<Option<Uuid>> {
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
