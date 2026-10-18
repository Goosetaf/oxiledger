use std::collections::HashSet;

use dioxus::prelude::ServerFnError;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    milestones::{self, Milestone},
    models::{
        account::AccountType,
        setup::{account_key, SetupRequest, SetupResult, MAX_SETUP_ACCOUNTS},
    },
};

fn db_error(err: sqlx::Error) -> ServerFnError {
    ServerFnError::new(err.to_string())
}

/// Apply everything the setup wizard collected in a single database transaction.
/// Anything that already exists is skipped and taken codes are dropped, so it can safely run again.
pub async fn apply_setup(
    pool: &PgPool,
    user_id: Uuid,
    req: SetupRequest,
) -> Result<SetupResult, ServerFnError> {
    if req.accounts.len() > MAX_SETUP_ACCOUNTS {
        return Err(ServerFnError::new(format!(
            "Setup can create at most {MAX_SETUP_ACCOUNTS} accounts at a time"
        )));
    }

    for account in &req.accounts {
        let name = account.name.trim();
        if name.is_empty() {
            return Err(ServerFnError::new("Account name cannot be empty"));
        }
        if name.chars().count() > 255 {
            return Err(ServerFnError::new(format!(
                "Account name \"{name}\" is too long"
            )));
        }
        if let Some(code) = account.code.as_deref() {
            if code.trim().chars().count() > 20 {
                return Err(ServerFnError::new(format!(
                    "Account code \"{}\" is too long",
                    code.trim()
                )));
            }
        }
    }

    let bank_account = match req.bank_account.as_ref() {
        Some(bank_account) => {
            let name = bank_account.name.trim().to_string();
            if name.is_empty() {
                return Err(ServerFnError::new("Bank account name is required"));
            }
            if name.chars().count() > 255 {
                return Err(ServerFnError::new("Bank account name is too long"));
            }

            let currency = bank_account.currency.trim().to_uppercase();
            if currency.is_empty() {
                return Err(ServerFnError::new("Currency is required"));
            }
            if currency.chars().count() > 10 {
                return Err(ServerFnError::new("Currency is too long"));
            }

            let linked_account_name = bank_account
                .linked_account_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string);

            Some((name, currency, linked_account_name))
        }
        None => None,
    };

    let mut tx = pool.begin().await.map_err(db_error)?;

    // Lock the user's row so two setup requests can't create duplicate accounts.
    sqlx::query!("SELECT id FROM users WHERE id = $1 FOR UPDATE", user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;

    let existing = sqlx::query!(
        r#"SELECT name, code, account_type AS "account_type: AccountType"
           FROM accounts
           WHERE user_id = $1"#,
        user_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(db_error)?;

    let mut taken_names: HashSet<(String, AccountType)> = existing
        .iter()
        .map(|row| account_key(&row.name, row.account_type))
        .collect();
    let mut taken_codes: HashSet<String> =
        existing.into_iter().filter_map(|row| row.code).collect();

    let mut created_accounts = 0u32;
    let mut skipped_accounts = 0u32;

    for account in &req.accounts {
        let name = account.name.trim();
        if !taken_names.insert(account_key(name, account.account_type)) {
            skipped_accounts += 1;
            continue;
        }

        let code = account
            .code
            .as_deref()
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .filter(|code| taken_codes.insert(code.to_string()));
        let description = account
            .description
            .as_deref()
            .map(str::trim)
            .filter(|description| !description.is_empty());

        sqlx::query!(
            "INSERT INTO accounts (user_id, name, code, account_type, normal_balance, description)
             VALUES ($1, $2, $3, $4, $5, $6)",
            user_id,
            name,
            code,
            account.account_type as _,
            account.account_type.default_normal_balance() as _,
            description
        )
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;

        created_accounts += 1;
    }

    let mut bank_account_id = None;
    let mut bank_account_existed = false;

    if let Some((name, currency, linked_account_name)) = bank_account {
        // Reuse a bank account with the same name, so running setup again doesn't add another
        let existing_id = sqlx::query_scalar!(
            "SELECT id FROM bank_accounts
             WHERE user_id = $1 AND LOWER(TRIM(name)) = LOWER($2)
             ORDER BY created_at
             LIMIT 1",
            user_id,
            name
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_error)?;

        bank_account_existed = existing_id.is_some();
        let id = match existing_id {
            Some(id) => id,
            None => {
                let internal_account_id = match linked_account_name {
                    // An exact match wins over one that only differs in case
                    Some(linked_name) => sqlx::query_scalar!(
                        "SELECT id FROM accounts
                         WHERE user_id = $1 AND account_type = 'Asset' AND LOWER(name) = LOWER($2)
                         ORDER BY name = $2 DESC, created_at
                         LIMIT 1",
                        user_id,
                        linked_name
                    )
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db_error)?,
                    None => None,
                };

                sqlx::query_scalar!(
                    "INSERT INTO bank_accounts (user_id, internal_account_id, name, currency, is_manual)
                     VALUES ($1, $2, $3, $4, TRUE)
                     RETURNING id",
                    user_id,
                    internal_account_id,
                    name,
                    currency
                )
                .fetch_one(&mut *tx)
                .await
                .map_err(db_error)?
            }
        };

        bank_account_id = Some(id);
    }

    milestones::record(&mut *tx, user_id, Milestone::SetupFinished)
        .await
        .map_err(db_error)?;

    tx.commit().await.map_err(db_error)?;

    Ok(SetupResult {
        created_accounts,
        skipped_accounts,
        bank_account_id,
        bank_account_existed,
    })
}
