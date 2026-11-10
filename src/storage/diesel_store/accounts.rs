use std::collections::HashMap;

use async_trait::async_trait;
use diesel::dsl::{not, sum};
use diesel::prelude::*;
use rust_decimal::Decimal;
use uuid::Uuid;

use super::coalesce;
use super::connection::DbConnection;
use super::schema::{accounts, journal_entries, transactions};
use super::types::{Id, Money, Timestamp};
use crate::models::account::{Account, AccountType, NormalBalance};
use crate::models::transaction::{AccountBalance, EntryType};
use crate::storage::accounts::{AccountDraft, AccountRepository};
use crate::storage::StorageResult;

super::store!(AccountStore);

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

#[async_trait]
impl AccountRepository for AccountStore {
    async fn list(&self, user_id: Uuid) -> StorageResult<Vec<Account>> {
        self.db
            .run(move |conn| Ok(load_accounts(conn, user_id, false)?))
            .await
    }

    async fn list_active(&self, user_id: Uuid) -> StorageResult<Vec<Account>> {
        self.db
            .run(move |conn| Ok(load_accounts(conn, user_id, true)?))
            .await
    }

    async fn find(&self, user_id: Uuid, id: Uuid) -> StorageResult<Option<Account>> {
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

    async fn create(&self, user_id: Uuid, draft: AccountDraft) -> StorageResult<Account> {
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

    async fn update(&self, user_id: Uuid, id: Uuid, draft: AccountDraft) -> StorageResult<bool> {
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

    async fn toggle_active(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
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

    async fn delete(&self, user_id: Uuid, id: Uuid) -> StorageResult<bool> {
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

    async fn balances(&self, user_id: Uuid) -> StorageResult<Vec<AccountBalance>> {
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

    async fn balance(&self, user_id: Uuid, id: Uuid) -> StorageResult<Decimal> {
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
}
