//! Seed data for local development and testing, written by `cargo seed`.
//!
//! Seeding replaces the seeded users and everything they own with the contents
//! of [`dataset`], so running it again resets them to a known state. It never
//! touches a user it did not create.

mod dataset;

use std::fmt::{self, Display};

use chrono::{DateTime, Days, Local, NaiveDate, TimeDelta, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    models::bank_sync::BankTransactionStatus,
    server::{auth, db},
};
use dataset::{BankAccount, Dataset, MAPPING_CONNECTIONS};

/// Owns the household described by [`dataset`].
const DEV_USER: &str = "dev";
/// Has no data at all, for working on empty states.
const EMPTY_USER: &str = "dev-empty";
/// Signed up through single sign-on, so it has no password to log in with.
const SSO_USER: &str = "dev-sso";
const SEEDED_USERS: [&str; 3] = [DEV_USER, EMPTY_USER, SSO_USER];
const PASSWORD: &str = "dev-password";

/// Entry point for `cargo seed`: migrates the database, then resets the seeded users.
pub fn run() {
    let result = tokio::runtime::Runtime::new()
        .expect("Failed to start the Tokio runtime")
        .block_on(async {
            let pool = db::connect().await;
            db::run_migrations(&pool).await;
            seed(&pool, Local::now()).await
        });

    match result {
        Ok(summary) => println!("{summary}"),
        Err(err) => {
            eprintln!("Seeding failed: {err}");
            std::process::exit(1);
        }
    }
}

enum SeedError {
    Database(sqlx::Error),
    /// A user the seeder did not create already has a seeded user's username or e-mail address.
    UserExists(String),
}

impl From<sqlx::Error> for SeedError {
    fn from(err: sqlx::Error) -> Self {
        Self::Database(err)
    }
}

impl Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(err) => write!(f, "{err}"),
            Self::UserExists(username) => write!(
                f,
                "the user `{username}` has the username or e-mail address of a seeded user but was \
                 not created by `cargo seed`. Rename or remove that user first; the seeder only \
                 replaces users it created."
            ),
        }
    }
}

/// The tables the seeder writes to.
#[derive(Debug, Clone, Copy)]
enum Row {
    User,
    Account,
    BankConnection,
    BankAccount,
    BankTransaction,
    Transaction,
    JournalEntry,
}

impl Row {
    /// Ids are derived from what a row is instead of being generated, so a
    /// reset keeps them and every URL that contains one. Rows without a name
    /// of their own are numbered, which holds until the dates move on. It is
    /// also how the seeder recognises the users it created.
    fn id(self, name: impl Display) -> Uuid {
        // FNV-1a over the table and the row's name within it.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in format!("{self:?}/{name}").bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        Uuid::from_u128((0x5eed_u128 << 112) | u128::from(hash))
    }
}

async fn seed(pool: &PgPool, now: DateTime<Local>) -> Result<Summary, SeedError> {
    let today = now.date_naive();
    let data = Dataset::household(today);
    let password_hash = auth::hash_password(PASSWORD).expect("Failed to hash the seed password");

    let mut tx = pool.begin().await?;

    delete_seeded_users(&mut tx).await?;

    for username in [DEV_USER, EMPTY_USER] {
        sqlx::query!(
            "INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)",
            Row::User.id(username),
            username,
            email(username),
            password_hash
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!(
        "INSERT INTO users (id, username, email, oidc_provider, oidc_subject)
         VALUES ($1, $2, $3, '1', $4)",
        Row::User.id(SSO_USER),
        SSO_USER,
        email(SSO_USER),
        format!("seed-{SSO_USER}")
    )
    .execute(&mut *tx)
    .await?;

    write_household(
        &mut tx,
        Row::User.id(DEV_USER),
        &data,
        now.with_timezone(&Utc),
    )
    .await?;

    tx.commit().await?;

    Ok(Summary {
        today,
        accounts: data.accounts.len(),
        transactions: data.transactions.len(),
        bank_accounts: data.bank_accounts.len(),
        pending: data.bank_transaction_count(BankTransactionStatus::Pending),
        linked: data.bank_transaction_count(BankTransactionStatus::Linked),
        dismissed: data.bank_transaction_count(BankTransactionStatus::Dismissed),
    })
}

fn email(username: &str) -> String {
    format!("{username}@oxiledger.test")
}

/// Removes the users an earlier run created. A user that only shares a
/// username or e-mail address with one of them was not created here, so
/// seeding stops instead of replacing it.
async fn delete_seeded_users(tx: &mut Transaction<'_, Postgres>) -> Result<(), SeedError> {
    let usernames = SEEDED_USERS.map(str::to_string);
    let emails = SEEDED_USERS.map(email);
    let user_ids = SEEDED_USERS.map(|username| Row::User.id(username));

    let someone_else = sqlx::query_scalar!(
        "SELECT username FROM users
         WHERE (username = ANY($1) OR email = ANY($2)) AND id <> ALL($3)
         LIMIT 1",
        &usernames[..],
        &emails[..],
        &user_ids[..]
    )
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(username) = someone_else {
        return Err(SeedError::UserExists(username));
    }

    // Deleting a user cascades to everything it owns, but journal entries and
    // bank accounts point at ledger accounts without a cascade and would block
    // it, so they have to go first.
    sqlx::query!(
        "DELETE FROM transactions WHERE user_id = ANY($1)",
        &user_ids[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "DELETE FROM bank_accounts WHERE user_id = ANY($1)",
        &user_ids[..]
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!("DELETE FROM users WHERE id = ANY($1)", &user_ids[..])
        .execute(&mut **tx)
        .await?;

    Ok(())
}

/// Writes the dataset as rows owned by `user_id`, leaving them in the state
/// the app itself would have produced.
async fn write_household(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    data: &Dataset,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    for account in &data.accounts {
        sqlx::query!(
            "INSERT INTO accounts
                (id, user_id, name, code, account_type, normal_balance, description, is_active)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            Row::Account.id(account.key),
            user_id,
            account.name,
            account.code,
            account.account_type as _,
            account.account_type.default_normal_balance() as _,
            account.description,
            account.is_active
        )
        .execute(&mut **tx)
        .await?;
    }

    for connection in &data.bank_connections {
        sqlx::query!(
            "INSERT INTO bank_connections
                (id, user_id, provider_id, provider_session_id, aspsp_name, aspsp_country,
                 access_valid_until, provider_accounts_json)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            Row::BankConnection.id(connection.key),
            user_id,
            connection.provider_id,
            format!("seed-{}-session", connection.key),
            connection.aspsp_name,
            connection.aspsp_country,
            now + connection.access_valid_for,
            serde_json::to_value(&connection.provider_accounts)
                .expect("provider accounts serialize to JSON")
        )
        .execute(&mut **tx)
        .await?;
    }

    for bank_account in &data.bank_accounts {
        let connected = bank_account.connection.as_ref();
        let synced_at = synced_at(bank_account, now);
        sqlx::query!(
            "INSERT INTO bank_accounts
                (id, user_id, bank_connection_id, internal_account_id, provider_account_uid,
                 iban, name, currency, is_manual, balance, last_synced_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            Row::BankAccount.id(bank_account.key),
            user_id,
            connected.map(|connected| Row::BankConnection.id(connected.connection)),
            bank_account.ledger_account.map(|key| Row::Account.id(key)),
            connected.map(|connected| connected.provider_account_uid),
            bank_account.iban,
            bank_account.name,
            bank_account.currency,
            connected.is_none(),
            synced_at.map(|_| data.bank_balance(bank_account.key)),
            synced_at
        )
        .execute(&mut **tx)
        .await?;
    }

    // A connection can only name the manual account it was started from once
    // that account exists.
    for connection in &data.bank_connections {
        if let Some(source) = connection.source_bank_account {
            sqlx::query!(
                "UPDATE bank_connections SET source_bank_account_id = $1 WHERE id = $2",
                Row::BankAccount.id(source),
                Row::BankConnection.id(connection.key)
            )
            .execute(&mut **tx)
            .await?;
        }
    }

    for (index, txn) in data.bank_transactions.iter().enumerate() {
        let bank_account = data.bank_account(txn.bank_account);
        let external_id = match &bank_account.connection {
            Some(connected) => format!("{}_{index}", connected.provider_account_uid),
            None => format!("manual-seed-{index}"),
        };
        // Transactions on a manual account are typed in, so "now" stands in for a sync.
        let last_synced = synced_at(bank_account, now).unwrap_or(now);
        sqlx::query!(
            "INSERT INTO bank_transactions
                (id, bank_account_id, user_id, external_id, date, amount, currency,
                 description, reference, status, imported_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            Row::BankTransaction.id(index),
            Row::BankAccount.id(txn.bank_account),
            user_id,
            external_id,
            txn.date,
            txn.amount,
            bank_account.currency,
            txn.description,
            txn.reference,
            txn.status.as_str(),
            imported_at(txn.date, last_synced, index)
        )
        .execute(&mut **tx)
        .await?;
    }

    for (txn_index, txn) in data.transactions.iter().enumerate() {
        let txn_id = Row::Transaction.id(txn_index);
        sqlx::query!(
            "INSERT INTO transactions (id, user_id, date, description, reference)
             VALUES ($1, $2, $3, $4, $5)",
            txn_id,
            user_id,
            txn.date,
            txn.description,
            txn.reference
        )
        .execute(&mut **tx)
        .await?;

        for (line_index, line) in txn.lines.iter().enumerate() {
            let entry_id = Row::JournalEntry.id(format!("{txn_index}.{line_index}"));
            let bank_txn_id = line.bank_txn.map(|index| Row::BankTransaction.id(index));
            sqlx::query!(
                "INSERT INTO journal_entries
                    (id, transaction_id, account_id, entry_type, amount, memo, bank_transaction_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
                entry_id,
                txn_id,
                Row::Account.id(line.account),
                line.entry_type as _,
                line.amount,
                line.memo,
                bank_txn_id
            )
            .execute(&mut **tx)
            .await?;

            // The two rows point at each other, so the bank transaction can
            // only name its journal entry once that exists.
            if let Some(bank_txn_id) = bank_txn_id {
                sqlx::query!(
                    "UPDATE bank_transactions SET linked_journal_entry_id = $1 WHERE id = $2",
                    entry_id,
                    bank_txn_id
                )
                .execute(&mut **tx)
                .await?;
            }
        }
    }

    Ok(())
}

/// When a connected bank account was last synced, if it ever was.
fn synced_at(bank_account: &BankAccount, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let connected = bank_account.connection.as_ref()?;
    Some(now - connected.last_synced_ago?)
}

/// A sync picks a transaction up the morning after it is booked, or at the
/// account's last sync if that came first. `sequence` keeps transactions that
/// share an import time in a stable order.
fn imported_at(date: NaiveDate, last_synced: DateTime<Utc>, sequence: usize) -> DateTime<Utc> {
    let next_morning = (date + Days::new(1))
        .and_hms_opt(6, 0, 0)
        .expect("06:00 is a valid time")
        .and_utc();

    next_morning.min(last_synced - TimeDelta::minutes(1)) + TimeDelta::milliseconds(sequence as i64)
}

struct Summary {
    today: NaiveDate,
    accounts: usize,
    transactions: usize,
    bank_accounts: usize,
    pending: usize,
    linked: usize,
    dismissed: usize,
}

impl Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            today,
            accounts,
            transactions,
            bank_accounts,
            pending,
            linked,
            dismissed,
        } = self;
        let bank_transactions = pending + linked + dismissed;
        let (no_password, width) = ("(none)", PASSWORD.len());

        write!(
            f,
            "Seeded test data as of {today}.

Log in with:
  {DEV_USER:<10} {PASSWORD}   a household with months of history
  {EMPTY_USER:<10} {PASSWORD}   no data, for empty states
  {SSO_USER:<10} {no_password:<width$}   single sign-on only, password login is rejected

{DEV_USER} owns {accounts} accounts, {transactions} transactions, {bank_accounts} bank accounts and
{bank_transactions} bank transactions ({pending} pending, {linked} linked, {dismissed} dismissed).

Bank connections waiting on the mapping page, which the app only opens
after a provider callback:"
        )?;
        for (key, shows) in MAPPING_CONNECTIONS {
            let id = Row::BankConnection.id(key);
            write!(
                f,
                "\n  /bank-accounts/connect/map?connection_id={id}   {shows}"
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The README lists these URLs, so the ids behind them must not drift.
    #[test]
    fn mapping_page_ids_match_the_readme() {
        let ids = MAPPING_CONNECTIONS.map(|(key, _)| Row::BankConnection.id(key).to_string());

        assert_eq!(
            ids,
            [
                "5eed0000-0000-0000-c84a-292dd8abaa46",
                "5eed0000-0000-0000-a5eb-0449632b6e84",
                "5eed0000-0000-0000-419e-ab9186cd0a44",
            ]
        );
    }
}
