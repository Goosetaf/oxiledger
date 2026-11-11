use std::collections::HashMap;

use async_trait::async_trait;
use chrono::NaiveDate;
use diesel::dsl::{exists, not, sum};
use diesel::prelude::*;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::coalesce;
use super::connection::DbConnection;
use super::schema::{accounts, bank_transactions, journal_entries, transactions};
use super::types::{Id, Money, Timestamp};
use crate::models::account::{Account, AccountType, NormalBalance};
use crate::models::transaction::{
    AccountBalance, EntryType, JournalEntry, JournalEntryInput, Transaction,
};
use crate::storage::ledger::{AccountDraft, LedgerRepository, TransactionDraft};
use crate::storage::{StorageError, StorageResult};

super::store!(LedgerStore);

#[derive(Queryable, Selectable)]
#[diesel(table_name = accounts)]
struct AccountRow {
    id: Id,
    user_id: Id,
    name: String,
    code: Option<String>,
    account_type: AccountType,
    normal_balance: NormalBalance,
    description: Option<String>,
    is_active: bool,
}

impl From<AccountRow> for Account {
    fn from(row: AccountRow) -> Self {
        Account {
            id: row.id.0,
            user_id: row.user_id.0,
            name: row.name,
            code: row.code,
            account_type: row.account_type,
            normal_balance: row.normal_balance,
            description: row.description,
            is_active: row.is_active,
        }
    }
}

fn type_rank(account_type: AccountType) -> u8 {
    match account_type {
        AccountType::Asset => 0,
        AccountType::Liability => 1,
        AccountType::Equity => 2,
        AccountType::Revenue => 3,
        AccountType::Expense => 4,
    }
}

fn load_accounts(
    conn: &mut DbConnection,
    user_id: Uuid,
    active_only: bool,
) -> QueryResult<Vec<Account>> {
    let mut query = accounts::table
        .filter(accounts::user_id.eq(Id(user_id)))
        .select(AccountRow::as_select())
        .order((coalesce(accounts::code, ""), accounts::name))
        .into_boxed();

    if active_only {
        query = query.filter(accounts::is_active.eq(true));
    }

    let mut accounts: Vec<Account> = query.load(conn)?.into_iter().map(Account::from).collect();
    // The stored type sorts by name on SQLite and by declaration on Postgres, so the order by
    // type is applied here, on top of the database's order by code and name.
    accounts.sort_by_key(|account| type_rank(account.account_type));
    Ok(accounts)
}

/// Debits minus credits, from sums grouped by entry type.
fn net_balance(debits: Option<Decimal>, credits: Option<Decimal>) -> Decimal {
    debits.unwrap_or_default() - credits.unwrap_or_default()
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = transactions)]
struct TransactionRow {
    id: Id,
    user_id: Id,
    date: NaiveDate,
    description: String,
    reference: Option<String>,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = journal_entries)]
struct EntryRow {
    id: Id,
    transaction_id: Id,
    account_id: Id,
    entry_type: EntryType,
    amount: Money,
    memo: Option<String>,
    bank_transaction_id: Option<Id>,
}

type JoinedRow = (TransactionRow, EntryRow, String, Option<String>);

fn entry(row: EntryRow, account_name: String, account_code: Option<String>) -> JournalEntry {
    JournalEntry {
        id: row.id.0,
        transaction_id: row.transaction_id.0,
        account_id: row.account_id.0,
        account_name: Some(account_name),
        account_code,
        entry_type: row.entry_type,
        amount: row.amount.0,
        memo: row.memo,
        bank_transaction_id: row.bank_transaction_id.map(|id| id.0),
    }
}

/// Folds rows that arrive grouped by transaction into transactions with their entries.
fn group_by_transaction(rows: Vec<JoinedRow>) -> Vec<Transaction> {
    let mut grouped: Vec<Transaction> = Vec::new();

    for (header, entry_row, account_name, account_code) in rows {
        let entry = entry(entry_row, account_name, account_code);
        match grouped.last_mut() {
            Some(current) if current.id == header.id.0 => current.entries.push(entry),
            _ => grouped.push(Transaction {
                id: header.id.0,
                user_id: header.user_id.0,
                date: header.date,
                description: header.description,
                reference: header.reference,
                entries: vec![entry],
            }),
        }
    }

    grouped
}

fn insert_entries(
    conn: &mut DbConnection,
    user_id: Uuid,
    transaction_id: Uuid,
    entries: &[JournalEntryInput],
    now: Timestamp,
) -> StorageResult<()> {
    for entry in entries {
        let entry_id = Id::new();

        diesel::insert_into(journal_entries::table)
            .values((
                journal_entries::id.eq(entry_id),
                journal_entries::transaction_id.eq(Id(transaction_id)),
                journal_entries::account_id.eq(Id(entry.account_id)),
                journal_entries::entry_type.eq(entry.entry_type),
                journal_entries::amount.eq(Money(entry.amount)),
                journal_entries::memo.eq(&entry.memo),
                journal_entries::bank_transaction_id.eq(entry.bank_transaction_id.map(Id)),
                journal_entries::created_at.eq(now),
            ))
            .execute(conn)?;

        if let Some(bank_transaction_id) = entry.bank_transaction_id {
            let linked = diesel::update(
                bank_transactions::table
                    .filter(bank_transactions::id.eq(Id(bank_transaction_id)))
                    .filter(bank_transactions::user_id.eq(Id(user_id)))
                    .filter(bank_transactions::status.eq("pending")),
            )
            .set((
                bank_transactions::status.eq("linked"),
                bank_transactions::linked_journal_entry_id.eq(Some(entry_id)),
            ))
            .execute(conn)?;

            if linked == 0 {
                return Err(StorageError::Conflict(
                    "A bank transaction is not available to link".to_string(),
                ));
            }
        }
    }

    Ok(())
}

/// Returns the bank transactions linked to a transaction's entries to pending.
fn unlink_bank_transactions(
    conn: &mut DbConnection,
    user_id: Uuid,
    transaction_id: Uuid,
) -> QueryResult<()> {
    let entries = journal_entries::table
        .filter(journal_entries::transaction_id.eq(Id(transaction_id)))
        .select((journal_entries::id, journal_entries::bank_transaction_id))
        .load::<(Id, Option<Id>)>(conn)?;

    let entry_ids = entries.iter().map(|(id, _)| *id);
    let named_by_entries = entries.iter().filter_map(|(_, linked)| *linked);

    // The link is recorded on both sides; either one counts.
    diesel::update(
        bank_transactions::table
            .filter(bank_transactions::user_id.eq(Id(user_id)))
            .filter(
                bank_transactions::id
                    .eq_any(named_by_entries)
                    .or(bank_transactions::linked_journal_entry_id.eq_any(entry_ids)),
            ),
    )
    .set((
        bank_transactions::status.eq("pending"),
        bank_transactions::linked_journal_entry_id.eq(None::<Id>),
    ))
    .execute(conn)?;

    Ok(())
}

#[async_trait]
impl LedgerRepository for LedgerStore {
    async fn list_accounts(&self, user_id: Uuid) -> StorageResult<Vec<Account>> {
        self.db
            .run(move |conn| Ok(load_accounts(conn, user_id, false)?))
            .await
    }

    async fn list_active_accounts(&self, user_id: Uuid) -> StorageResult<Vec<Account>> {
        self.db
            .run(move |conn| Ok(load_accounts(conn, user_id, true)?))
            .await
    }

    async fn find_account(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<Account>> {
        self.db
            .run(move |conn| {
                let row = accounts::table
                    .filter(accounts::id.eq(Id(id)))
                    .filter(accounts::user_id.eq(Id(user_id)))
                    .select(AccountRow::as_select())
                    .first(conn)
                    .optional()?;
                Ok(row.map(Account::from))
            })
            .await
    }

    async fn create_account(&self, user_id: Uuid, draft: AccountDraft) -> StorageResult<Account> {
        self.db
            .run(move |conn| {
                let now = Timestamp::now();
                let account = Account {
                    id: Uuid::new_v4(),
                    user_id,
                    name: draft.name,
                    code: draft.code,
                    account_type: draft.account_type,
                    normal_balance: draft.normal_balance,
                    description: draft.description,
                    is_active: true,
                };

                diesel::insert_into(accounts::table)
                    .values((
                        accounts::id.eq(Id(account.id)),
                        accounts::user_id.eq(Id(account.user_id)),
                        accounts::name.eq(&account.name),
                        accounts::code.eq(&account.code),
                        accounts::account_type.eq(account.account_type),
                        accounts::normal_balance.eq(account.normal_balance),
                        accounts::description.eq(&account.description),
                        accounts::is_active.eq(account.is_active),
                        accounts::created_at.eq(now),
                        accounts::updated_at.eq(now),
                    ))
                    .execute(conn)?;

                Ok(account)
            })
            .await
    }

    async fn update_account(
        &self,
        user_id: Uuid,
        id: Uuid,
        draft: AccountDraft,
    ) -> StorageResult<bool> {
        self.db
            .run(move |conn| {
                let updated = diesel::update(
                    accounts::table
                        .filter(accounts::id.eq(Id(id)))
                        .filter(accounts::user_id.eq(Id(user_id))),
                )
                .set((
                    accounts::name.eq(draft.name),
                    accounts::code.eq(draft.code),
                    accounts::account_type.eq(draft.account_type),
                    accounts::normal_balance.eq(draft.normal_balance),
                    accounts::description.eq(draft.description),
                    accounts::updated_at.eq(Timestamp::now()),
                ))
                .execute(conn)?;
                Ok(updated > 0)
            })
            .await
    }

    async fn toggle_account_active(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
        self.db
            .run(move |conn| {
                let updated = diesel::update(
                    accounts::table
                        .filter(accounts::id.eq(Id(id)))
                        .filter(accounts::user_id.eq(Id(user_id))),
                )
                .set((
                    accounts::is_active.eq(not(accounts::is_active)),
                    accounts::updated_at.eq(Timestamp::now()),
                ))
                .execute(conn)?;
                Ok(updated > 0)
            })
            .await
    }

    async fn delete_account(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
        self.db
            .run(move |conn| {
                let deleted = diesel::delete(
                    accounts::table
                        .filter(accounts::id.eq(Id(id)))
                        .filter(accounts::user_id.eq(Id(user_id))),
                )
                .execute(conn)?;
                Ok(deleted > 0)
            })
            .await
    }

    async fn account_balances(&self, user_id: Uuid) -> StorageResult<Vec<AccountBalance>> {
        self.db
            .run(move |conn| {
                let accounts = load_accounts(conn, user_id, true)?;

                let sums = journal_entries::table
                    .inner_join(accounts::table)
                    .filter(accounts::user_id.eq(Id(user_id)))
                    .filter(accounts::is_active.eq(true))
                    .group_by((journal_entries::account_id, journal_entries::entry_type))
                    .select((
                        journal_entries::account_id,
                        journal_entries::entry_type,
                        sum(journal_entries::amount),
                    ))
                    .load::<(Id, EntryType, Option<Money>)>(conn)?;

                let mut totals: HashMap<(Uuid, EntryType), Decimal> = HashMap::new();
                for (account_id, entry_type, total) in sums {
                    if let Some(total) = total {
                        totals.insert((account_id.0, entry_type), total.0);
                    }
                }

                Ok(accounts
                    .into_iter()
                    .map(|account| AccountBalance {
                        balance: net_balance(
                            totals.get(&(account.id, EntryType::Debit)).copied(),
                            totals.get(&(account.id, EntryType::Credit)).copied(),
                        ),
                        account_id: account.id,
                        account_name: account.name,
                        account_code: account.code,
                    })
                    .collect())
            })
            .await
    }

    async fn account_balance(&self, user_id: Uuid, id: Uuid) -> StorageResult<Decimal> {
        self.db
            .run(move |conn| {
                let sums = journal_entries::table
                    .inner_join(transactions::table)
                    .filter(journal_entries::account_id.eq(Id(id)))
                    .filter(transactions::user_id.eq(Id(user_id)))
                    .group_by(journal_entries::entry_type)
                    .select((journal_entries::entry_type, sum(journal_entries::amount)))
                    .load::<(EntryType, Option<Money>)>(conn)?;

                let total = |wanted: EntryType| {
                    sums.iter()
                        .find(|(entry_type, _)| *entry_type == wanted)
                        .and_then(|(_, total)| total.map(|money| money.0))
                };

                Ok(net_balance(
                    total(EntryType::Debit),
                    total(EntryType::Credit),
                ))
            })
            .await
    }

    async fn list_transactions(&self, user_id: Uuid) -> StorageResult<Vec<Transaction>> {
        self.db
            .run(move |conn| {
                let rows = transactions::table
                    .inner_join(journal_entries::table.inner_join(accounts::table))
                    .filter(transactions::user_id.eq(Id(user_id)))
                    .select((
                        TransactionRow::as_select(),
                        EntryRow::as_select(),
                        accounts::name,
                        accounts::code,
                    ))
                    .order((transactions::date.desc(), transactions::id.asc()))
                    .load::<JoinedRow>(conn)?;

                let mut transactions = group_by_transaction(rows);
                for transaction in &mut transactions {
                    transaction
                        .entries
                        .sort_by_key(|entry| entry.entry_type == EntryType::Credit);
                }
                Ok(transactions)
            })
            .await
    }

    async fn find_transaction(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<Transaction>> {
        self.db
            .run(move |conn| {
                let rows = transactions::table
                    .inner_join(journal_entries::table.inner_join(accounts::table))
                    .filter(transactions::id.eq(Id(id)))
                    .filter(transactions::user_id.eq(Id(user_id)))
                    .select((
                        TransactionRow::as_select(),
                        EntryRow::as_select(),
                        accounts::name,
                        accounts::code,
                    ))
                    .order((journal_entries::created_at.asc(), journal_entries::id.asc()))
                    .load::<JoinedRow>(conn)?;

                Ok(group_by_transaction(rows).pop())
            })
            .await
    }

    async fn create_transaction(
        &self,
        user_id: Uuid,
        draft: TransactionDraft,
    ) -> StorageResult<Uuid> {
        self.db
            .transaction(move |conn| {
                let now = Timestamp::now();
                let id = Uuid::new_v4();

                diesel::insert_into(transactions::table)
                    .values((
                        transactions::id.eq(Id(id)),
                        transactions::user_id.eq(Id(user_id)),
                        transactions::date.eq(draft.date),
                        transactions::description.eq(draft.description),
                        transactions::reference.eq(draft.reference),
                        transactions::created_at.eq(now),
                        transactions::updated_at.eq(now),
                    ))
                    .execute(conn)?;

                insert_entries(conn, user_id, id, &draft.entries, now)?;
                Ok(id)
            })
            .await
    }

    async fn update_transaction(
        &self,
        user_id: Uuid,
        id: Uuid,
        draft: TransactionDraft,
    ) -> StorageResult<bool> {
        self.db
            .transaction(move |conn| {
                let now = Timestamp::now();

                let updated = diesel::update(
                    transactions::table
                        .filter(transactions::id.eq(Id(id)))
                        .filter(transactions::user_id.eq(Id(user_id))),
                )
                .set((
                    transactions::date.eq(draft.date),
                    transactions::description.eq(draft.description),
                    transactions::reference.eq(draft.reference),
                    transactions::updated_at.eq(now),
                ))
                .execute(conn)?;

                if updated == 0 {
                    return Ok(false);
                }

                // Frees every bank transaction first; the new entries link theirs again.
                unlink_bank_transactions(conn, user_id, id)?;
                diesel::delete(
                    journal_entries::table.filter(journal_entries::transaction_id.eq(Id(id))),
                )
                .execute(conn)?;

                insert_entries(conn, user_id, id, &draft.entries, now)?;
                Ok(true)
            })
            .await
    }

    async fn delete_transaction(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
        self.db
            .transaction(move |conn| {
                let owned = transactions::table
                    .filter(transactions::id.eq(Id(id)))
                    .filter(transactions::user_id.eq(Id(user_id)));

                if !diesel::select(exists(owned.select(transactions::id))).get_result(conn)? {
                    return Ok(false);
                }

                unlink_bank_transactions(conn, user_id, id)?;
                diesel::delete(owned).execute(conn)?;
                Ok(true)
            })
            .await
    }
}
