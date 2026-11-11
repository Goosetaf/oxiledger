//! Written by hand, because the column types are the portable ones from `types`. Keep it in
//! step with the migrations of both databases.

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    users (id) {
        id -> DbUuid,
        username -> Text,
        email -> Text,
        password_hash -> Nullable<Text>,
        oidc_provider -> Nullable<Text>,
        oidc_subject -> Nullable<Text>,
        preferences -> DbJson,
        created_at -> DbTimestamp,
        updated_at -> DbTimestamp,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    sessions (id) {
        id -> DbUuid,
        user_id -> DbUuid,
        token -> Text,
        expires_at -> DbTimestamp,
        created_at -> DbTimestamp,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    oidc_states (id) {
        id -> DbUuid,
        provider_id -> Text,
        csrf_token -> Text,
        pkce_verifier -> Text,
        nonce -> Text,
        expires_at -> DbTimestamp,
        created_at -> DbTimestamp,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    accounts (id) {
        id -> DbUuid,
        user_id -> DbUuid,
        name -> Text,
        code -> Nullable<Text>,
        account_type -> DbAccountType,
        normal_balance -> DbNormalBalance,
        description -> Nullable<Text>,
        is_active -> Bool,
        created_at -> DbTimestamp,
        updated_at -> DbTimestamp,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    transactions (id) {
        id -> DbUuid,
        user_id -> DbUuid,
        date -> Date,
        description -> Text,
        reference -> Nullable<Text>,
        created_at -> DbTimestamp,
        updated_at -> DbTimestamp,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    journal_entries (id) {
        id -> DbUuid,
        transaction_id -> DbUuid,
        account_id -> DbUuid,
        entry_type -> DbEntryType,
        amount -> DbMoney,
        memo -> Nullable<Text>,
        bank_transaction_id -> Nullable<DbUuid>,
        created_at -> DbTimestamp,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    bank_connections (id) {
        id -> DbUuid,
        user_id -> DbUuid,
        provider_id -> Text,
        provider_session_id -> Text,
        aspsp_name -> Nullable<Text>,
        aspsp_country -> Nullable<Text>,
        access_valid_until -> Nullable<DbTimestamp>,
        provider_accounts_json -> Nullable<DbJson>,
        source_bank_account_id -> Nullable<DbUuid>,
        created_at -> DbTimestamp,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    bank_accounts (id) {
        id -> DbUuid,
        user_id -> DbUuid,
        bank_connection_id -> Nullable<DbUuid>,
        internal_account_id -> Nullable<DbUuid>,
        provider_account_uid -> Nullable<Text>,
        iban -> Nullable<Text>,
        name -> Nullable<Text>,
        currency -> Text,
        is_manual -> Bool,
        balance -> Nullable<DbMoney>,
        last_synced_at -> Nullable<DbTimestamp>,
        created_at -> DbTimestamp,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    bank_transactions (id) {
        id -> DbUuid,
        bank_account_id -> DbUuid,
        user_id -> DbUuid,
        external_id -> Text,
        date -> Date,
        amount -> DbMoney,
        currency -> Text,
        description -> Text,
        reference -> Nullable<Text>,
        status -> Text,
        linked_journal_entry_id -> Nullable<DbUuid>,
        imported_at -> DbTimestamp,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use crate::storage::diesel_store::types::*;

    bank_auth_states (id) {
        id -> DbUuid,
        user_id -> DbUuid,
        provider_id -> Text,
        state -> Text,
        aspsp_name -> Nullable<Text>,
        aspsp_country -> Nullable<Text>,
        provider_reference -> Nullable<Text>,
        source_bank_account_id -> Nullable<DbUuid>,
        expires_at -> DbTimestamp,
        created_at -> DbTimestamp,
    }
}

diesel::joinable!(journal_entries -> transactions (transaction_id));
diesel::joinable!(journal_entries -> accounts (account_id));
diesel::joinable!(bank_accounts -> bank_connections (bank_connection_id));
diesel::joinable!(bank_accounts -> accounts (internal_account_id));
diesel::joinable!(bank_transactions -> bank_accounts (bank_account_id));

diesel::allow_tables_to_appear_in_same_query!(
    users,
    sessions,
    oidc_states,
    accounts,
    transactions,
    journal_entries,
    bank_connections,
    bank_accounts,
    bank_transactions,
    bank_auth_states,
);
