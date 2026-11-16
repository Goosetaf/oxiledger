use async_trait::async_trait;
use chrono::NaiveDate;
use diesel::prelude::*;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::coalesce;
use super::connection::on_backend;
use super::schema::{accounts, bank_accounts, bank_transactions, journal_entries};
use super::types::{Id, Money, Timestamp};
use crate::models::bank_sync::{
    BankTransaction, BankTransactionOverview, BankTransactionPrefill, SyncResult,
};
use crate::storage::bank_transactions::{
    BankTransactionLinkInfo, BankTransactionRepository, NewBankTransaction,
};
use crate::storage::StorageResult;

super::store!(BankTransactionStore);

#[derive(Queryable, Selectable)]
#[diesel(table_name = bank_transactions)]
struct BankTransactionRow {
    id: Id,
    bank_account_id: Id,
    user_id: Id,
    external_id: String,
    date: NaiveDate,
    amount: Money,
    currency: String,
    description: String,
    reference: Option<String>,
    status: String,
    linked_journal_entry_id: Option<Id>,
    imported_at: Timestamp,
}

impl From<BankTransactionRow> for BankTransaction {
    fn from(row: BankTransactionRow) -> Self {
        BankTransaction {
            id: row.id.0,
            bank_account_id: row.bank_account_id.0,
            user_id: row.user_id.0,
            external_id: row.external_id,
            date: row.date,
            amount: row.amount.0,
            currency: row.currency,
            description: row.description,
            reference: row.reference,
            status: row.status,
            linked_journal_entry_id: row.linked_journal_entry_id.map(|id| id.0),
            imported_at: row.imported_at.0,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = bank_transactions)]
struct NewBankTransactionRow {
    id: Id,
    bank_account_id: Id,
    user_id: Id,
    external_id: String,
    date: NaiveDate,
    amount: Money,
    currency: String,
    description: String,
    reference: Option<String>,
    status: &'static str,
    imported_at: Timestamp,
}

impl NewBankTransactionRow {
    fn pending(
        user_id: Uuid,
        bank_account_id: Uuid,
        transaction: NewBankTransaction,
        imported_at: Timestamp,
    ) -> Self {
        Self {
            id: Id::new(),
            bank_account_id: Id(bank_account_id),
            user_id: Id(user_id),
            external_id: transaction.external_id,
            date: transaction.date,
            amount: Money(transaction.amount),
            currency: transaction.currency,
            description: transaction.description,
            reference: transaction.reference,
            status: "pending",
            imported_at,
        }
    }
}

type PrefillRow = (
    Id,
    NaiveDate,
    Money,
    String,
    Option<String>,
    Option<Id>,
    Option<String>,
    String,
);

type PrefillColumns = (
    bank_transactions::id,
    bank_transactions::date,
    bank_transactions::amount,
    bank_transactions::description,
    bank_transactions::reference,
    bank_accounts::internal_account_id,
    diesel::dsl::Nullable<accounts::name>,
    bank_transactions::currency,
);

fn prefill_columns() -> PrefillColumns {
    (
        bank_transactions::id,
        bank_transactions::date,
        bank_transactions::amount,
        bank_transactions::description,
        bank_transactions::reference,
        bank_accounts::internal_account_id,
        accounts::name.nullable(),
        bank_transactions::currency,
    )
}

fn prefill(row: PrefillRow) -> BankTransactionPrefill {
    let (
        id,
        date,
        amount,
        description,
        reference,
        internal_account_id,
        internal_account_name,
        currency,
    ) = row;

    BankTransactionPrefill {
        bank_transaction_id: id.0,
        date,
        amount: amount.0,
        description,
        reference,
        internal_account_id: internal_account_id.map(|id| id.0),
        internal_account_name,
        currency,
    }
}

#[async_trait]
impl BankTransactionRepository for BankTransactionStore {
    async fn list_pending(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
    ) -> StorageResult<Vec<BankTransaction>> {
        self.db
            .run(move |conn| {
                let rows = bank_transactions::table
                    .filter(bank_transactions::bank_account_id.eq(Id(bank_account_id)))
                    .filter(bank_transactions::user_id.eq(Id(user_id)))
                    .filter(bank_transactions::status.eq("pending"))
                    .select(BankTransactionRow::as_select())
                    .order((
                        bank_transactions::date.desc(),
                        bank_transactions::imported_at.desc(),
                    ))
                    .load(conn)?;
                Ok(rows.into_iter().map(BankTransaction::from).collect())
            })
            .await
    }

    async fn list_overview(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
    ) -> StorageResult<Vec<BankTransactionOverview>> {
        type Row = (
            Id,
            NaiveDate,
            Money,
            String,
            String,
            Option<String>,
            String,
            Option<Id>,
            Timestamp,
        );

        self.db
            .run(move |conn| {
                let rows = bank_transactions::table
                    .left_join(
                        journal_entries::table.on(journal_entries::id
                            .nullable()
                            .eq(bank_transactions::linked_journal_entry_id)),
                    )
                    .filter(bank_transactions::bank_account_id.eq(Id(bank_account_id)))
                    .filter(bank_transactions::user_id.eq(Id(user_id)))
                    .select((
                        bank_transactions::id,
                        bank_transactions::date,
                        bank_transactions::amount,
                        bank_transactions::currency,
                        bank_transactions::description,
                        bank_transactions::reference,
                        bank_transactions::status,
                        journal_entries::transaction_id.nullable(),
                        bank_transactions::imported_at,
                    ))
                    .order((
                        bank_transactions::date.desc(),
                        bank_transactions::imported_at.desc(),
                    ))
                    .load::<Row>(conn)?;

                Ok(rows
                    .into_iter()
                    .map(
                        |(
                            id,
                            date,
                            amount,
                            currency,
                            description,
                            reference,
                            status,
                            linked_transaction_id,
                            imported_at,
                        )| BankTransactionOverview {
                            id: id.0,
                            date,
                            amount: amount.0,
                            currency,
                            description,
                            reference,
                            status,
                            linked_transaction_id: linked_transaction_id.map(|id| id.0),
                            imported_at: imported_at.0,
                        },
                    )
                    .collect())
            })
            .await
    }

    async fn create(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
        transaction: NewBankTransaction,
    ) -> StorageResult<()> {
        self.db
            .run(move |conn| {
                let row = NewBankTransactionRow::pending(
                    user_id,
                    bank_account_id,
                    transaction,
                    Timestamp::now(),
                );
                diesel::insert_into(bank_transactions::table)
                    .values(&row)
                    .execute(conn)?;
                Ok(())
            })
            .await
    }

    async fn import(
        &self,
        user_id: Uuid,
        bank_account_id: Uuid,
        transactions: Vec<NewBankTransaction>,
        balance: Option<Decimal>,
    ) -> StorageResult<SyncResult> {
        self.db
            .transaction(move |conn| {
                let now = Timestamp::now();
                let mut result = SyncResult {
                    new_count: 0,
                    skipped_count: 0,
                };

                for transaction in transactions {
                    let row =
                        NewBankTransactionRow::pending(user_id, bank_account_id, transaction, now);

                    let inserted =
                        on_backend!(conn, |c| diesel::insert_into(bank_transactions::table)
                            .values(&row)
                            .on_conflict((
                                bank_transactions::bank_account_id,
                                bank_transactions::external_id,
                            ))
                            .do_nothing()
                            .execute(c))?;

                    if inserted > 0 {
                        result.new_count += 1;
                    } else {
                        result.skipped_count += 1;
                    }
                }

                diesel::update(bank_accounts::table.find(Id(bank_account_id)))
                    .set((
                        bank_accounts::last_synced_at.eq(Some(now)),
                        bank_accounts::balance.eq(balance.map(Money)),
                    ))
                    .execute(conn)?;

                Ok(result)
            })
            .await
    }

    async fn dismiss(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
        self.set_status(user_id, id, "pending", "dismissed").await
    }

    async fn restore(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
        self.set_status(user_id, id, "dismissed", "pending").await
    }

    async fn find_pending_prefill(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Option<BankTransactionPrefill>> {
        self.db
            .run(move |conn| {
                let row = bank_transactions::table
                    .inner_join(bank_accounts::table.left_join(accounts::table))
                    .filter(bank_transactions::id.eq(Id(id)))
                    .filter(bank_transactions::user_id.eq(Id(user_id)))
                    .filter(bank_transactions::status.eq("pending"))
                    .select(prefill_columns())
                    .first::<PrefillRow>(conn)
                    .optional()?;
                Ok(row.map(prefill))
            })
            .await
    }

    async fn list_pending_prefills(
        &self,
        user_id: Uuid,
        date: NaiveDate,
    ) -> StorageResult<Vec<BankTransactionPrefill>> {
        self.db
            .run(move |conn| {
                let rows = bank_transactions::table
                    .inner_join(bank_accounts::table.left_join(accounts::table))
                    .filter(bank_transactions::user_id.eq(Id(user_id)))
                    .filter(bank_transactions::status.eq("pending"))
                    .filter(bank_transactions::date.eq(date))
                    .select(prefill_columns())
                    .order((
                        coalesce(accounts::code, ""),
                        coalesce(accounts::name.nullable(), ""),
                        bank_transactions::amount.desc(),
                        bank_transactions::imported_at.desc(),
                    ))
                    .load::<PrefillRow>(conn)?;
                Ok(rows.into_iter().map(prefill).collect())
            })
            .await
    }

    async fn list_transfer_matches(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> StorageResult<Vec<BankTransactionPrefill>> {
        self.db
            .run(move |conn| {
                let source = bank_transactions::table
                    .filter(bank_transactions::id.eq(Id(id)))
                    .filter(bank_transactions::user_id.eq(Id(user_id)))
                    .filter(bank_transactions::status.eq("pending"))
                    .select((
                        bank_transactions::bank_account_id,
                        bank_transactions::date,
                        bank_transactions::amount,
                    ))
                    .first::<(Id, NaiveDate, Money)>(conn)
                    .optional()?;

                let Some((source_account_id, date, amount)) = source else {
                    return Ok(Vec::new());
                };

                let rows = bank_transactions::table
                    .inner_join(bank_accounts::table.left_join(accounts::table))
                    .filter(bank_transactions::user_id.eq(Id(user_id)))
                    .filter(bank_transactions::status.eq("pending"))
                    .filter(bank_transactions::id.ne(Id(id)))
                    .filter(bank_transactions::bank_account_id.ne(source_account_id))
                    .filter(bank_transactions::date.eq(date))
                    .filter(bank_transactions::amount.eq(Money(-amount.0)))
                    .select(prefill_columns())
                    .order((
                        coalesce(accounts::code, ""),
                        coalesce(accounts::name.nullable(), ""),
                        bank_transactions::imported_at.desc(),
                    ))
                    .load::<PrefillRow>(conn)?;
                Ok(rows.into_iter().map(prefill).collect())
            })
            .await
    }

    async fn link_info(
        &self,
        user_id: Uuid,
        ids: Vec<Uuid>,
    ) -> StorageResult<Vec<BankTransactionLinkInfo>> {
        type Row = (Id, NaiveDate, Money, String, Option<Id>, Option<Id>);

        self.db
            .run(move |conn| {
                let rows = bank_transactions::table
                    .inner_join(bank_accounts::table)
                    .left_join(
                        journal_entries::table.on(journal_entries::id
                            .nullable()
                            .eq(bank_transactions::linked_journal_entry_id)),
                    )
                    .filter(bank_transactions::id.eq_any(ids.into_iter().map(Id)))
                    .filter(bank_transactions::user_id.eq(Id(user_id)))
                    .select((
                        bank_transactions::id,
                        bank_transactions::date,
                        bank_transactions::amount,
                        bank_transactions::status,
                        bank_accounts::internal_account_id,
                        journal_entries::transaction_id.nullable(),
                    ))
                    .load::<Row>(conn)?;

                Ok(rows
                    .into_iter()
                    .map(
                        |(id, date, amount, status, internal_account_id, linked_transaction_id)| {
                            BankTransactionLinkInfo {
                                id: id.0,
                                date,
                                amount: amount.0,
                                status,
                                internal_account_id: internal_account_id.map(|id| id.0),
                                linked_transaction_id: linked_transaction_id.map(|id| id.0),
                            }
                        },
                    )
                    .collect())
            })
            .await
    }
}

impl BankTransactionStore {
    async fn set_status(
        &self,
        user_id: Uuid,
        id: Uuid,
        from: &'static str,
        to: &'static str,
    ) -> StorageResult<bool> {
        self.db
            .run(move |conn| {
                let updated = diesel::update(
                    bank_transactions::table
                        .filter(bank_transactions::id.eq(Id(id)))
                        .filter(bank_transactions::user_id.eq(Id(user_id)))
                        .filter(bank_transactions::status.eq(from)),
                )
                .set(bank_transactions::status.eq(to))
                .execute(conn)?;
                Ok(updated > 0)
            })
            .await
    }
}
