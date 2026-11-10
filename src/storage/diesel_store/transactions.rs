use async_trait::async_trait;
use chrono::NaiveDate;
use diesel::dsl::exists;
use diesel::prelude::*;
use uuid::Uuid;

use super::connection::DbConnection;
use super::schema::{accounts, bank_transactions, journal_entries, transactions};
use super::types::{Id, Money, Timestamp};
use crate::models::transaction::{EntryType, JournalEntry, JournalEntryInput, Transaction};
use crate::storage::transactions::{TransactionDraft, TransactionRepository};
use crate::storage::{StorageError, StorageResult};

super::store!(TransactionStore);

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
impl TransactionRepository for TransactionStore {
    async fn list(&self, user_id: Uuid) -> StorageResult<Vec<Transaction>> {
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

    async fn find(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<Transaction>> {
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

    async fn create(&self, user_id: Uuid, draft: TransactionDraft) -> StorageResult<Uuid> {
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

    async fn update(
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

    async fn delete(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
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
