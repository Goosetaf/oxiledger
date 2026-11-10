//! What every storage backend must do. Each test runs against SQLite, and against Postgres too
//! when `OXILEDGER_TEST_POSTGRES_URL` is set.

use std::future::Future;

use chrono::{Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::bank::{
    BankAccountDetails, BankAuthState, NewBankConnection, NewBankTransaction, ProviderAccount,
};
use super::diesel_store::test_support;
use super::ledger::{AccountDraft, TransactionDraft};
use super::users::{NewUser, OidcIdentity, OidcState};
use super::{Storage, StorageError};
use crate::bank_sync::provider::ProviderBankAccount;
use crate::models::account::{AccountType, NormalBalance};
use crate::models::transaction::{EntryType, JournalEntryInput};
use crate::navigation::Destination;
use crate::preferences::Preferences;

async fn on_each_database<F, Fut>(test: F)
where
    F: Fn(Storage) -> Fut,
    Fut: Future<Output = ()>,
{
    let sqlite = test_support::sqlite().await;
    test(sqlite.storage.clone()).await;

    if let Some(postgres) = test_support::postgres().await {
        test(postgres.storage.clone()).await;
    }
}

fn dec(value: &str) -> Decimal {
    value.parse().unwrap()
}

fn day(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, day).unwrap()
}

fn password_user(name: &str) -> NewUser {
    NewUser {
        username: name.to_string(),
        email: format!("{name}@example.com"),
        password_hash: Some("hash".to_string()),
        oidc: None,
    }
}

async fn user(storage: &Storage, name: &str) -> Uuid {
    storage.users.create(password_user(name)).await.unwrap()
}

fn account_draft(name: &str, code: Option<&str>, account_type: AccountType) -> AccountDraft {
    AccountDraft {
        name: name.to_string(),
        code: code.map(str::to_string),
        account_type,
        normal_balance: account_type.default_normal_balance(),
        description: None,
    }
}

async fn account(storage: &Storage, user_id: Uuid, name: &str, kind: AccountType) -> Uuid {
    storage
        .ledger
        .create_account(user_id, account_draft(name, None, kind))
        .await
        .unwrap()
        .id
}

fn entry(account_id: Uuid, entry_type: EntryType, amount: &str) -> JournalEntryInput {
    JournalEntryInput {
        account_id,
        entry_type,
        amount: dec(amount),
        memo: None,
        bank_transaction_id: None,
    }
}

fn linked_entry(
    account_id: Uuid,
    entry_type: EntryType,
    amount: &str,
    bank_transaction_id: Uuid,
) -> JournalEntryInput {
    JournalEntryInput {
        bank_transaction_id: Some(bank_transaction_id),
        ..entry(account_id, entry_type, amount)
    }
}

fn draft(date: NaiveDate, description: &str, entries: Vec<JournalEntryInput>) -> TransactionDraft {
    TransactionDraft {
        date,
        description: description.to_string(),
        reference: None,
        entries,
    }
}

fn bank_details(name: &str, internal_account_id: Option<Uuid>) -> BankAccountDetails {
    BankAccountDetails {
        name: name.to_string(),
        iban: None,
        currency: "EUR".to_string(),
        internal_account_id,
    }
}

fn bank_transaction(external_id: &str, date: NaiveDate, amount: &str) -> NewBankTransaction {
    NewBankTransaction {
        external_id: external_id.to_string(),
        date,
        amount: dec(amount),
        currency: "EUR".to_string(),
        description: format!("Payment {external_id}"),
        reference: None,
    }
}

fn provider_account(uid: &str, name: Option<&str>) -> ProviderAccount {
    ProviderAccount {
        uid: uid.to_string(),
        name: name.map(str::to_string),
        iban: None,
        currency: "SEK".to_string(),
    }
}

async fn connection(storage: &Storage, user_id: Uuid, source: Option<Uuid>) -> Uuid {
    storage
        .bank
        .create_connection(NewBankConnection {
            user_id,
            provider_id: "enable_banking".to_string(),
            provider_session_id: "session-1".to_string(),
            aspsp_name: Some("Nordea".to_string()),
            aspsp_country: Some("SE".to_string()),
            access_valid_until: Utc::now() + Duration::days(90),
            provider_accounts: vec![ProviderBankAccount {
                uid: "uid-1".to_string(),
                name: Some("Salary".to_string()),
                iban: None,
                currency: "SEK".to_string(),
            }],
            source_bank_account_id: source,
        })
        .await
        .unwrap()
}

/// A bank account holding one pending bank transaction, returning both ids.
async fn pending_bank_transaction(
    storage: &Storage,
    user_id: Uuid,
    internal_account_id: Option<Uuid>,
    amount: &str,
) -> (Uuid, Uuid) {
    let bank_account_id = storage
        .bank
        .create_manual_account(user_id, bank_details("Checking", internal_account_id))
        .await
        .unwrap();
    storage
        .bank
        .create_transaction(
            user_id,
            bank_account_id,
            bank_transaction(&Uuid::new_v4().to_string(), day(5), amount),
        )
        .await
        .unwrap();
    let pending = storage
        .bank
        .list_pending_transactions(user_id, bank_account_id)
        .await
        .unwrap();
    (bank_account_id, pending[0].id)
}

async fn status_of(storage: &Storage, user_id: Uuid, bank_account_id: Uuid, id: Uuid) -> String {
    storage
        .bank
        .list_transaction_overview(user_id, bank_account_id)
        .await
        .unwrap()
        .into_iter()
        .find(|transaction| transaction.id == id)
        .unwrap()
        .status
}

#[tokio::test]
async fn only_one_first_user_can_be_created() {
    on_each_database(|storage| async move {
        assert!(!storage.users.any_exist().await.unwrap());

        let attempts = (0..8).map(|n| {
            let storage = storage.clone();
            tokio::spawn(async move {
                storage
                    .users
                    .create_first(password_user(&format!("first{n}")))
                    .await
                    .unwrap()
            })
        });

        let mut created = 0;
        for attempt in attempts.collect::<Vec<_>>() {
            if attempt.await.unwrap().is_some() {
                created += 1;
            }
        }

        assert_eq!(created, 1);
        assert!(storage.users.any_exist().await.unwrap());
    })
    .await;
}

#[tokio::test]
async fn users_are_found_by_username_and_names_stay_unique() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;

        let credentials = storage
            .users
            .find_credentials("ada")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(credentials.id, id);
        assert_eq!(credentials.password_hash.as_deref(), Some("hash"));
        assert!(storage
            .users
            .find_credentials("bob")
            .await
            .unwrap()
            .is_none());

        assert!(storage
            .users
            .is_taken("ada", "other@example.com")
            .await
            .unwrap());
        assert!(storage
            .users
            .is_taken("other", "ada@example.com")
            .await
            .unwrap());
        assert!(!storage
            .users
            .is_taken("other", "other@example.com")
            .await
            .unwrap());

        let duplicate = storage.users.create(password_user("ada")).await;
        assert!(matches!(duplicate, Err(StorageError::UniqueViolation(_))));
    })
    .await;
}

#[tokio::test]
async fn preferences_are_saved_per_user() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;

        let current = storage.users.find_current(id).await.unwrap().unwrap();
        assert_eq!(current.username, "ada");
        assert_eq!(current.preferences, Preferences::default());

        let mut preferences = Preferences::default();
        preferences.navigation.move_later(Destination::Dashboard);
        storage
            .users
            .save_preferences(id, &preferences)
            .await
            .unwrap();

        let current = storage.users.find_current(id).await.unwrap().unwrap();
        assert_eq!(current.preferences, preferences);
        assert!(storage
            .users
            .find_current(Uuid::new_v4())
            .await
            .unwrap()
            .is_none());
    })
    .await;
}

#[tokio::test]
async fn an_oidc_identity_maps_to_one_user_whose_email_is_refreshed() {
    on_each_database(|storage| async move {
        let identity = OidcIdentity {
            provider: "1".to_string(),
            subject: "subject-1".to_string(),
        };
        assert!(!storage.users.has_oidc_identity(&identity).await.unwrap());

        let id = storage
            .users
            .upsert_oidc("ada", "old@example.com", &identity)
            .await
            .unwrap();
        assert!(storage.users.has_oidc_identity(&identity).await.unwrap());

        let again = storage
            .users
            .upsert_oidc("renamed", "new@example.com", &identity)
            .await
            .unwrap();
        assert_eq!(again, id);
        assert!(storage
            .users
            .is_taken("-", "new@example.com")
            .await
            .unwrap());
        assert!(!storage
            .users
            .is_taken("-", "old@example.com")
            .await
            .unwrap());
        assert!(!storage.users.is_taken("renamed", "-").await.unwrap());

        let other_identity = OidcIdentity {
            provider: "1".to_string(),
            subject: "subject-2".to_string(),
        };
        let clash = storage
            .users
            .upsert_oidc("ada", "third@example.com", &other_identity)
            .await;
        assert!(matches!(clash, Err(StorageError::UniqueViolation(_))));
    })
    .await;
}

#[tokio::test]
async fn sessions_resolve_to_their_user_until_they_expire_or_are_deleted() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;

        storage
            .users
            .create_session(id, "live", Utc::now() + Duration::days(1))
            .await
            .unwrap();
        storage
            .users
            .create_session(id, "expired", Utc::now() - Duration::seconds(1))
            .await
            .unwrap();

        assert_eq!(
            storage.users.find_session_user("live").await.unwrap(),
            Some(id)
        );
        assert_eq!(
            storage.users.find_session_user("expired").await.unwrap(),
            None
        );
        assert_eq!(
            storage.users.find_session_user("unknown").await.unwrap(),
            None
        );

        storage.users.delete_session("live").await.unwrap();
        assert_eq!(storage.users.find_session_user("live").await.unwrap(), None);
    })
    .await;
}

#[tokio::test]
async fn an_oidc_state_can_be_consumed_once_and_only_before_it_expires() {
    on_each_database(|storage| async move {
        let state = |csrf_token: &str, expires_in: Duration| OidcState {
            provider_id: "1".to_string(),
            csrf_token: csrf_token.to_string(),
            pkce_verifier: "verifier".to_string(),
            nonce: "nonce".to_string(),
            expires_at: Utc::now() + expires_in,
        };
        let users = &storage.users;

        users
            .create_oidc_state(state("live", Duration::minutes(10)))
            .await
            .unwrap();
        users
            .create_oidc_state(state("expired", Duration::seconds(-1)))
            .await
            .unwrap();

        let consumed = users.consume_oidc_state("live").await.unwrap().unwrap();
        assert_eq!(consumed.provider_id, "1");
        assert_eq!(consumed.pkce_verifier, "verifier");
        assert_eq!(consumed.nonce, "nonce");

        assert!(users.consume_oidc_state("live").await.unwrap().is_none());
        assert!(users.consume_oidc_state("expired").await.unwrap().is_none());
    })
    .await;
}

#[tokio::test]
async fn accounts_are_listed_by_type_then_code_then_name() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let other = user(&storage, "bob").await;

        for (name, code, kind) in [
            ("Groceries", Some("5000"), AccountType::Expense),
            ("Salary", Some("4000"), AccountType::Revenue),
            ("Savings", Some("1100"), AccountType::Asset),
            ("Cash", Some("1000"), AccountType::Asset),
            ("Loan", None, AccountType::Liability),
            ("Opening balance", None, AccountType::Equity),
        ] {
            storage
                .ledger
                .create_account(id, account_draft(name, code, kind))
                .await
                .unwrap();
        }
        storage
            .ledger
            .create_account(other, account_draft("Not mine", None, AccountType::Asset))
            .await
            .unwrap();

        let names = |list: Vec<crate::models::account::Account>| {
            list.into_iter()
                .map(|account| account.name)
                .collect::<Vec<_>>()
        };

        let listed = storage.ledger.list_accounts(id).await.unwrap();
        assert_eq!(listed[0].normal_balance, NormalBalance::Debit);
        assert_eq!(
            names(listed.clone()),
            [
                "Cash",
                "Savings",
                "Loan",
                "Opening balance",
                "Salary",
                "Groceries"
            ]
        );

        assert!(storage
            .ledger
            .toggle_account_active(id, listed[1].id)
            .await
            .unwrap());
        assert!(!storage
            .ledger
            .toggle_account_active(other, listed[0].id)
            .await
            .unwrap());
        assert_eq!(
            names(storage.ledger.list_active_accounts(id).await.unwrap()),
            ["Cash", "Loan", "Opening balance", "Salary", "Groceries"]
        );
        assert!(
            !storage
                .ledger
                .find_account(id, listed[1].id)
                .await
                .unwrap()
                .unwrap()
                .is_active
        );
    })
    .await;
}

#[tokio::test]
async fn accounts_can_be_changed_only_by_their_owner_and_codes_stay_unique() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let other = user(&storage, "bob").await;

        let cash = storage
            .ledger
            .create_account(id, account_draft("Cash", Some("1000"), AccountType::Asset))
            .await
            .unwrap();
        assert!(cash.is_active);
        assert_eq!(
            storage.ledger.find_account(id, cash.id).await.unwrap(),
            Some(cash.clone())
        );
        assert_eq!(
            storage.ledger.find_account(other, cash.id).await.unwrap(),
            None
        );

        let duplicate = storage
            .ledger
            .create_account(
                id,
                account_draft("Wallet", Some("1000"), AccountType::Asset),
            )
            .await;
        assert!(matches!(duplicate, Err(StorageError::UniqueViolation(_))));
        storage
            .ledger
            .create_account(
                other,
                account_draft("Cash", Some("1000"), AccountType::Asset),
            )
            .await
            .unwrap();

        let mut changed = account_draft("Petty cash", None, AccountType::Liability);
        changed.description = Some("Coins".to_string());
        assert!(!storage
            .ledger
            .update_account(other, cash.id, changed.clone())
            .await
            .unwrap());
        assert!(storage
            .ledger
            .update_account(id, cash.id, changed)
            .await
            .unwrap());

        let found = storage
            .ledger
            .find_account(id, cash.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.name, "Petty cash");
        assert_eq!(found.code, None);
        assert_eq!(found.account_type, AccountType::Liability);
        assert_eq!(found.normal_balance, NormalBalance::Credit);
        assert_eq!(found.description.as_deref(), Some("Coins"));

        assert!(!storage.ledger.delete_account(other, cash.id).await.unwrap());
        assert!(storage.ledger.delete_account(id, cash.id).await.unwrap());
        assert_eq!(
            storage.ledger.find_account(id, cash.id).await.unwrap(),
            None
        );
    })
    .await;
}

#[tokio::test]
async fn an_account_with_journal_entries_cannot_be_deleted() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let cash = account(&storage, id, "Cash", AccountType::Asset).await;
        let food = account(&storage, id, "Food", AccountType::Expense).await;

        storage
            .ledger
            .create_transaction(
                id,
                draft(
                    day(1),
                    "Lunch",
                    vec![
                        entry(food, EntryType::Debit, "9.50"),
                        entry(cash, EntryType::Credit, "9.50"),
                    ],
                ),
            )
            .await
            .unwrap();

        let deleted = storage.ledger.delete_account(id, cash).await;
        assert!(matches!(deleted, Err(StorageError::ForeignKeyViolation(_))));
    })
    .await;
}

#[tokio::test]
async fn balances_are_exact_sums_of_debits_minus_credits() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let cash = account(&storage, id, "Cash", AccountType::Asset).await;
        let food = account(&storage, id, "Food", AccountType::Expense).await;
        let unused = account(&storage, id, "Unused", AccountType::Equity).await;
        let hidden = account(&storage, id, "Hidden", AccountType::Asset).await;
        storage
            .ledger
            .toggle_account_active(id, hidden)
            .await
            .unwrap();

        for amount in ["0.1", "0.2", "1234.5678"] {
            storage
                .ledger
                .create_transaction(
                    id,
                    draft(
                        day(1),
                        "Spend",
                        vec![
                            entry(food, EntryType::Debit, amount),
                            entry(cash, EntryType::Credit, amount),
                        ],
                    ),
                )
                .await
                .unwrap();
        }

        let balances = storage.ledger.account_balances(id).await.unwrap();
        let balance_of = |account_id: Uuid| {
            balances
                .iter()
                .find(|balance| balance.account_id == account_id)
                .map(|balance| balance.balance)
        };

        assert_eq!(balance_of(food), Some(dec("1234.8678")));
        assert_eq!(balance_of(cash), Some(dec("-1234.8678")));
        assert_eq!(balance_of(unused), Some(Decimal::ZERO));
        assert_eq!(balance_of(hidden), None);

        assert_eq!(
            storage.ledger.account_balance(id, food).await.unwrap(),
            dec("1234.8678")
        );
        assert_eq!(
            storage.ledger.account_balance(id, unused).await.unwrap(),
            Decimal::ZERO
        );
        let stranger = user(&storage, "bob").await;
        assert_eq!(
            storage
                .ledger
                .account_balance(stranger, food)
                .await
                .unwrap(),
            Decimal::ZERO
        );
    })
    .await;
}

#[tokio::test]
async fn transactions_keep_their_entries_and_list_newest_first() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let other = user(&storage, "bob").await;
        let cash = account(&storage, id, "Cash", AccountType::Asset).await;
        let food = account(&storage, id, "Food", AccountType::Expense).await;

        let mut lunch = draft(
            day(1),
            "Lunch",
            vec![
                entry(cash, EntryType::Credit, "12.34"),
                entry(food, EntryType::Debit, "12.34"),
            ],
        );
        lunch.reference = Some("R-1".to_string());
        lunch.entries[0].memo = Some("Card".to_string());
        let lunch_id = storage.ledger.create_transaction(id, lunch).await.unwrap();

        let dinner_id = storage
            .ledger
            .create_transaction(
                id,
                draft(
                    day(2),
                    "Dinner",
                    vec![
                        entry(food, EntryType::Debit, "30"),
                        entry(cash, EntryType::Credit, "30"),
                    ],
                ),
            )
            .await
            .unwrap();

        let listed = storage.ledger.list_transactions(id).await.unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|transaction| transaction.id)
                .collect::<Vec<_>>(),
            [dinner_id, lunch_id]
        );
        assert!(listed.iter().all(|transaction| {
            transaction.entries.len() == 2
                && transaction.entries[0].entry_type == EntryType::Debit
                && transaction.entries[1].entry_type == EntryType::Credit
        }));
        assert!(storage
            .ledger
            .list_transactions(other)
            .await
            .unwrap()
            .is_empty());

        let found = storage
            .ledger
            .find_transaction(id, lunch_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.date, day(1));
        assert_eq!(found.description, "Lunch");
        assert_eq!(found.reference.as_deref(), Some("R-1"));
        let credit = found
            .entries
            .iter()
            .find(|entry| entry.entry_type == EntryType::Credit)
            .unwrap();
        assert_eq!(credit.account_id, cash);
        assert_eq!(credit.account_name.as_deref(), Some("Cash"));
        assert_eq!(credit.amount, dec("12.34"));
        assert_eq!(credit.memo.as_deref(), Some("Card"));
        assert!(storage
            .ledger
            .find_transaction(other, lunch_id)
            .await
            .unwrap()
            .is_none());

        let replacement = draft(
            day(3),
            "Late lunch",
            vec![
                entry(food, EntryType::Debit, "5"),
                entry(food, EntryType::Debit, "6"),
                entry(cash, EntryType::Credit, "11"),
            ],
        );
        assert!(!storage
            .ledger
            .update_transaction(other, lunch_id, replacement.clone())
            .await
            .unwrap());
        assert!(storage
            .ledger
            .update_transaction(id, lunch_id, replacement)
            .await
            .unwrap());

        let found = storage
            .ledger
            .find_transaction(id, lunch_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.date, day(3));
        assert_eq!(found.description, "Late lunch");
        assert_eq!(found.reference, None);
        assert_eq!(found.entries.len(), 3);

        assert!(!storage
            .ledger
            .delete_transaction(other, lunch_id)
            .await
            .unwrap());
        assert!(storage
            .ledger
            .delete_transaction(id, lunch_id)
            .await
            .unwrap());
        assert!(storage
            .ledger
            .find_transaction(id, lunch_id)
            .await
            .unwrap()
            .is_none());
        assert_eq!(storage.ledger.list_transactions(id).await.unwrap().len(), 1);
    })
    .await;
}

#[tokio::test]
async fn a_bank_transaction_is_linked_while_a_journal_entry_names_it() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let checking = account(&storage, id, "Checking", AccountType::Asset).await;
        let income = account(&storage, id, "Income", AccountType::Revenue).await;
        let (bank_account_id, bank_transaction_id) =
            pending_bank_transaction(&storage, id, Some(checking), "100").await;

        let linking = || {
            draft(
                day(5),
                "Salary",
                vec![
                    linked_entry(checking, EntryType::Debit, "100", bank_transaction_id),
                    entry(income, EntryType::Credit, "100"),
                ],
            )
        };
        let plain = || {
            draft(
                day(5),
                "Salary",
                vec![
                    entry(checking, EntryType::Debit, "100"),
                    entry(income, EntryType::Credit, "100"),
                ],
            )
        };

        let transaction_id = storage
            .ledger
            .create_transaction(id, linking())
            .await
            .unwrap();
        assert_eq!(
            status_of(&storage, id, bank_account_id, bank_transaction_id).await,
            "linked"
        );

        let info = storage
            .bank
            .transaction_link_info(id, vec![bank_transaction_id])
            .await
            .unwrap();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].internal_account_id, Some(checking));
        assert_eq!(info[0].linked_transaction_id, Some(transaction_id));
        assert_eq!(info[0].amount, dec("100"));

        let found = storage
            .ledger
            .find_transaction(id, transaction_id)
            .await
            .unwrap()
            .unwrap();
        assert!(found
            .entries
            .iter()
            .any(|entry| entry.bank_transaction_id == Some(bank_transaction_id)));

        // Saving again with the same link keeps it.
        assert!(storage
            .ledger
            .update_transaction(id, transaction_id, linking())
            .await
            .unwrap());
        assert_eq!(
            status_of(&storage, id, bank_account_id, bank_transaction_id).await,
            "linked"
        );

        // A second transaction cannot take it, and leaves nothing behind when it fails.
        let taken = storage.ledger.create_transaction(id, linking()).await;
        assert!(matches!(taken, Err(StorageError::Conflict(_))));
        assert_eq!(storage.ledger.list_transactions(id).await.unwrap().len(), 1);

        assert!(storage
            .ledger
            .update_transaction(id, transaction_id, plain())
            .await
            .unwrap());
        assert_eq!(
            status_of(&storage, id, bank_account_id, bank_transaction_id).await,
            "pending"
        );

        assert!(storage
            .ledger
            .update_transaction(id, transaction_id, linking())
            .await
            .unwrap());
        assert!(storage
            .ledger
            .delete_transaction(id, transaction_id)
            .await
            .unwrap());
        assert_eq!(
            status_of(&storage, id, bank_account_id, bank_transaction_id).await,
            "pending"
        );
        let info = storage
            .bank
            .transaction_link_info(id, vec![bank_transaction_id])
            .await
            .unwrap();
        assert_eq!(info[0].linked_transaction_id, None);
    })
    .await;
}

#[tokio::test]
async fn another_users_bank_transaction_cannot_be_linked() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let other = user(&storage, "bob").await;
        let checking = account(&storage, id, "Checking", AccountType::Asset).await;
        let income = account(&storage, id, "Income", AccountType::Revenue).await;
        let (_, foreign_bank_transaction_id) =
            pending_bank_transaction(&storage, other, None, "100").await;

        assert!(storage
            .bank
            .transaction_link_info(id, vec![foreign_bank_transaction_id])
            .await
            .unwrap()
            .is_empty());

        let created = storage
            .ledger
            .create_transaction(
                id,
                draft(
                    day(5),
                    "Salary",
                    vec![
                        linked_entry(
                            checking,
                            EntryType::Debit,
                            "100",
                            foreign_bank_transaction_id,
                        ),
                        entry(income, EntryType::Credit, "100"),
                    ],
                ),
            )
            .await;

        assert!(matches!(created, Err(StorageError::Conflict(_))));
        assert!(storage
            .ledger
            .list_transactions(id)
            .await
            .unwrap()
            .is_empty());
    })
    .await;
}

#[tokio::test]
async fn manual_bank_accounts_can_be_created_changed_and_deleted() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let other = user(&storage, "bob").await;
        let checking = account(&storage, id, "Checking", AccountType::Asset).await;

        let bank_account_id = storage
            .bank
            .create_manual_account(id, bank_details("Wallet", Some(checking)))
            .await
            .unwrap();

        let found = storage
            .bank
            .find_account(id, bank_account_id)
            .await
            .unwrap()
            .unwrap();
        assert!(found.is_manual);
        assert_eq!(found.name.as_deref(), Some("Wallet"));
        assert_eq!(found.currency, "EUR");
        assert_eq!(found.internal_account_id, Some(checking));
        assert_eq!(found.bank_connection_id, None);
        assert_eq!(found.balance, None);
        assert!(storage
            .bank
            .find_account(other, bank_account_id)
            .await
            .unwrap()
            .is_none());

        let summary = storage
            .bank
            .find_account_summary(id, bank_account_id)
            .await
            .unwrap()
            .unwrap();
        assert!(summary.is_manual);
        assert_eq!(summary.internal_account_name.as_deref(), Some("Checking"));
        assert_eq!(summary.provider_id, None);
        assert!(storage
            .bank
            .find_account_summary(other, bank_account_id)
            .await
            .unwrap()
            .is_none());

        let changed = BankAccountDetails {
            name: "Purse".to_string(),
            iban: Some("SE00".to_string()),
            currency: "SEK".to_string(),
            internal_account_id: None,
        };
        assert!(!storage
            .bank
            .update_account(other, bank_account_id, changed.clone())
            .await
            .unwrap());
        assert!(storage
            .bank
            .update_account(id, bank_account_id, changed)
            .await
            .unwrap());

        let found = storage
            .bank
            .find_account(id, bank_account_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.name.as_deref(), Some("Purse"));
        assert_eq!(found.iban.as_deref(), Some("SE00"));
        assert_eq!(found.currency, "SEK");
        assert_eq!(found.internal_account_id, None);

        assert!(!storage
            .bank
            .delete_account(other, bank_account_id)
            .await
            .unwrap());
        assert!(storage
            .bank
            .delete_account(id, bank_account_id)
            .await
            .unwrap());
        assert!(storage
            .bank
            .find_account(id, bank_account_id)
            .await
            .unwrap()
            .is_none());
    })
    .await;
}

#[tokio::test]
async fn bank_accounts_are_listed_by_bank_name_then_account_name() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let connection_id = connection(&storage, id, None).await;

        storage
            .bank
            .create_manual_account(id, bank_details("Zebra", None))
            .await
            .unwrap();
        storage
            .bank
            .create_manual_account(id, bank_details("Alpha", None))
            .await
            .unwrap();
        storage
            .bank
            .map_provider_account(
                id,
                connection_id,
                provider_account("uid-2", Some("Savings")),
                None,
            )
            .await
            .unwrap();
        storage
            .bank
            .map_provider_account(
                id,
                connection_id,
                provider_account("uid-1", Some("Card")),
                None,
            )
            .await
            .unwrap();
        storage
            .bank
            .map_provider_account(id, connection_id, provider_account("uid-3", None), None)
            .await
            .unwrap();

        let listed = storage.bank.list_accounts(id).await.unwrap();
        let names: Vec<_> = listed
            .iter()
            .map(|account| account.name.as_deref())
            .collect();
        // Connected accounts sort under their bank, "Nordea".
        assert_eq!(
            names,
            [
                Some("Alpha"),
                Some("Card"),
                Some("Savings"),
                None,
                Some("Zebra")
            ]
        );
        assert_eq!(listed[1].aspsp_name.as_deref(), Some("Nordea"));
        assert_eq!(listed[1].provider_id.as_deref(), Some("enable_banking"));
        assert!(listed[1].access_valid_until.is_some());
        assert!(!listed[1].is_manual);
    })
    .await;
}

#[tokio::test]
async fn mapping_a_provider_account_twice_updates_the_same_bank_account() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let checking = account(&storage, id, "Checking", AccountType::Asset).await;
        let connection_id = connection(&storage, id, None).await;
        let other_connection_id = connection(&storage, id, None).await;

        let first = storage
            .bank
            .map_provider_account(
                id,
                connection_id,
                provider_account("uid-1", Some("Salary")),
                None,
            )
            .await
            .unwrap();
        storage
            .bank
            .import_transactions(id, first, vec![], Some(dec("10")))
            .await
            .unwrap();

        let second = storage
            .bank
            .map_provider_account(
                id,
                connection_id,
                provider_account("uid-1", Some("Renamed")),
                Some(checking),
            )
            .await
            .unwrap();
        assert_eq!(second, first);

        let found = storage.bank.find_account(id, first).await.unwrap().unwrap();
        assert_eq!(found.name.as_deref(), Some("Renamed"));
        assert_eq!(found.internal_account_id, Some(checking));
        assert_eq!(found.balance, None);
        assert_eq!(found.last_synced_at, None);

        let elsewhere = storage
            .bank
            .map_provider_account(
                id,
                other_connection_id,
                provider_account("uid-1", None),
                None,
            )
            .await
            .unwrap();
        assert_ne!(elsewhere, first);

        assert_eq!(
            storage
                .bank
                .count_connection_accounts(connection_id)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            storage
                .bank
                .list_connection_accounts(id, connection_id)
                .await
                .unwrap()[0]
                .id,
            first
        );

        let mut linked = storage
            .bank
            .linked_provider_uids(
                id,
                vec!["uid-1".to_string(), "uid-9".to_string()],
                other_connection_id,
            )
            .await
            .unwrap();
        linked.sort();
        assert_eq!(linked, ["uid-1"]);
        storage.bank.delete_account(id, first).await.unwrap();
        assert!(storage
            .bank
            .linked_provider_uids(id, vec!["uid-1".to_string()], other_connection_id)
            .await
            .unwrap()
            .is_empty());
    })
    .await;
}

#[tokio::test]
async fn mapping_the_same_provider_account_at_once_yields_one_bank_account() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let connection_id = connection(&storage, id, None).await;

        let attempts = (0..8).map(|_| {
            let storage = storage.clone();
            tokio::spawn(async move {
                storage
                    .bank
                    .map_provider_account(id, connection_id, provider_account("uid-1", None), None)
                    .await
                    .unwrap()
            })
        });

        let mut mapped = Vec::new();
        for attempt in attempts.collect::<Vec<_>>() {
            mapped.push(attempt.await.unwrap());
        }

        assert!(mapped
            .iter()
            .all(|bank_account_id| *bank_account_id == mapped[0]));
        assert_eq!(
            storage
                .bank
                .count_connection_accounts(connection_id)
                .await
                .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn a_manual_bank_account_can_be_connected_and_disconnected() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let other = user(&storage, "bob").await;

        let bank_account_id = storage
            .bank
            .create_manual_account(id, bank_details("Wallet", None))
            .await
            .unwrap();
        let connection_id = connection(&storage, id, Some(bank_account_id)).await;

        let details = storage
            .bank
            .find_connection(id, connection_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(details.source_bank_account_id, Some(bank_account_id));
        assert_eq!(details.provider_accounts.len(), 1);
        assert_eq!(details.provider_accounts[0].uid, "uid-1");
        assert!(storage
            .bank
            .find_connection(other, connection_id)
            .await
            .unwrap()
            .is_none());

        assert!(storage
            .bank
            .list_sync_candidates()
            .await
            .unwrap()
            .is_empty());
        let syncable = storage
            .bank
            .find_syncable_account(bank_account_id, None)
            .await
            .unwrap()
            .unwrap();
        assert!(syncable.is_manual);
        assert_eq!(syncable.session, None);

        assert!(!storage
            .bank
            .connect_account(
                other,
                bank_account_id,
                connection_id,
                provider_account("uid-1", None)
            )
            .await
            .unwrap());
        assert!(storage
            .bank
            .connect_account(
                id,
                bank_account_id,
                connection_id,
                provider_account("uid-1", None)
            )
            .await
            .unwrap());

        let found = storage
            .bank
            .find_account(id, bank_account_id)
            .await
            .unwrap()
            .unwrap();
        assert!(!found.is_manual);
        assert_eq!(found.name.as_deref(), Some("Wallet"));
        assert_eq!(found.currency, "SEK");
        assert_eq!(found.provider_account_uid.as_deref(), Some("uid-1"));

        let syncable = storage
            .bank
            .find_syncable_account(bank_account_id, Some(id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(syncable.user_id, id);
        let session = syncable.session.unwrap();
        assert_eq!(session.connection_id, connection_id);
        assert_eq!(session.provider_id, "enable_banking");
        assert_eq!(session.provider_session_id, "session-1");
        assert!(storage
            .bank
            .find_syncable_account(bank_account_id, Some(other))
            .await
            .unwrap()
            .is_none());

        let candidates = storage.bank.list_sync_candidates().await.unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, bank_account_id);
        assert_eq!(candidates[0].last_synced_at, None);

        assert!(storage
            .bank
            .disconnect_account(id, bank_account_id)
            .await
            .unwrap());
        let found = storage
            .bank
            .find_account(id, bank_account_id)
            .await
            .unwrap()
            .unwrap();
        assert!(found.is_manual);
        assert_eq!(found.bank_connection_id, None);
        assert_eq!(found.provider_account_uid, None);
        assert_eq!(
            storage
                .bank
                .count_connection_accounts(connection_id)
                .await
                .unwrap(),
            0
        );

        assert!(!storage
            .bank
            .delete_connection(other, connection_id)
            .await
            .unwrap());
        assert!(storage
            .bank
            .delete_connection(id, connection_id)
            .await
            .unwrap());
        assert!(storage
            .bank
            .find_connection(id, connection_id)
            .await
            .unwrap()
            .is_none());
    })
    .await;
}

#[tokio::test]
async fn a_bank_auth_state_can_be_consumed_once_by_its_user() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let other = user(&storage, "bob").await;
        let source = storage
            .bank
            .create_manual_account(id, bank_details("Wallet", None))
            .await
            .unwrap();
        let bank = &storage.bank;

        let state = |state: &str, expires_in: Duration| BankAuthState {
            user_id: id,
            provider_id: "gocardless".to_string(),
            state: state.to_string(),
            aspsp_name: Some("Nordea".to_string()),
            aspsp_country: Some("SE".to_string()),
            provider_reference: Some("requisition".to_string()),
            source_bank_account_id: Some(source),
            expires_at: Utc::now() + expires_in,
        };

        bank.create_auth_state(state("live", Duration::minutes(15)))
            .await
            .unwrap();
        bank.create_auth_state(state("expired", Duration::seconds(-1)))
            .await
            .unwrap();

        assert!(bank
            .consume_auth_state(other, "live")
            .await
            .unwrap()
            .is_none());
        let consumed = bank.consume_auth_state(id, "live").await.unwrap().unwrap();
        assert_eq!(consumed.provider_id, "gocardless");
        assert_eq!(consumed.aspsp_name.as_deref(), Some("Nordea"));
        assert_eq!(consumed.provider_reference.as_deref(), Some("requisition"));
        assert_eq!(consumed.source_bank_account_id, Some(source));

        assert!(bank.consume_auth_state(id, "live").await.unwrap().is_none());
        assert!(bank
            .consume_auth_state(id, "expired")
            .await
            .unwrap()
            .is_none());
    })
    .await;
}

#[tokio::test]
async fn importing_skips_bank_transactions_seen_before() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let bank_account_id = storage
            .bank
            .create_manual_account(id, bank_details("Checking", None))
            .await
            .unwrap();

        let first = storage
            .bank
            .import_transactions(
                id,
                bank_account_id,
                vec![
                    bank_transaction("a", day(1), "10.00"),
                    bank_transaction("b", day(2), "-4.25"),
                ],
                Some(dec("105.75")),
            )
            .await
            .unwrap();
        assert_eq!((first.new_count, first.skipped_count), (2, 0));

        let before = Utc::now() - Duration::seconds(5);
        let second = storage
            .bank
            .import_transactions(
                id,
                bank_account_id,
                vec![
                    bank_transaction("b", day(2), "-4.25"),
                    bank_transaction("c", day(3), "1"),
                ],
                None,
            )
            .await
            .unwrap();
        assert_eq!((second.new_count, second.skipped_count), (1, 1));

        let account = storage
            .bank
            .find_account(id, bank_account_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.balance, None);
        assert!(account.last_synced_at.unwrap() > before);

        let pending = storage
            .bank
            .list_pending_transactions(id, bank_account_id)
            .await
            .unwrap();
        assert_eq!(
            pending
                .iter()
                .map(|transaction| transaction.external_id.as_str())
                .collect::<Vec<_>>(),
            ["c", "b", "a"]
        );
        assert_eq!(pending[1].amount, dec("-4.25"));
        assert_eq!(pending[1].status, "pending");
        assert_eq!(pending[1].user_id, id);

        let stranger = user(&storage, "bob").await;
        assert!(storage
            .bank
            .list_pending_transactions(stranger, bank_account_id)
            .await
            .unwrap()
            .is_empty());
    })
    .await;
}

#[tokio::test]
async fn bank_transactions_can_be_dismissed_and_restored() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let other = user(&storage, "bob").await;
        let (bank_account_id, bank_transaction_id) =
            pending_bank_transaction(&storage, id, None, "10").await;

        assert!(!storage
            .bank
            .restore_transaction(id, bank_transaction_id)
            .await
            .unwrap());
        assert!(!storage
            .bank
            .dismiss_transaction(other, bank_transaction_id)
            .await
            .unwrap());
        assert!(storage
            .bank
            .dismiss_transaction(id, bank_transaction_id)
            .await
            .unwrap());
        assert!(!storage
            .bank
            .dismiss_transaction(id, bank_transaction_id)
            .await
            .unwrap());

        assert!(storage
            .bank
            .list_pending_transactions(id, bank_account_id)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            status_of(&storage, id, bank_account_id, bank_transaction_id).await,
            "dismissed"
        );
        assert!(storage
            .bank
            .find_pending_prefill(id, bank_transaction_id)
            .await
            .unwrap()
            .is_none());

        assert!(storage
            .bank
            .restore_transaction(id, bank_transaction_id)
            .await
            .unwrap());
        assert_eq!(
            status_of(&storage, id, bank_account_id, bank_transaction_id).await,
            "pending"
        );
    })
    .await;
}

#[tokio::test]
async fn pending_bank_transactions_prefill_new_transactions() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let checking = account(&storage, id, "Checking", AccountType::Asset).await;
        let savings = account(&storage, id, "Savings", AccountType::Asset).await;

        let checking_bank = storage
            .bank
            .create_manual_account(id, bank_details("Checking", Some(checking)))
            .await
            .unwrap();
        let savings_bank = storage
            .bank
            .create_manual_account(id, bank_details("Savings", Some(savings)))
            .await
            .unwrap();
        let unlinked_bank = storage
            .bank
            .create_manual_account(id, bank_details("Unlinked", None))
            .await
            .unwrap();

        storage
            .bank
            .import_transactions(
                id,
                checking_bank,
                vec![
                    bank_transaction("out", day(7), "-250"),
                    bank_transaction("small", day(7), "-3"),
                    bank_transaction("other-day", day(8), "-250"),
                ],
                None,
            )
            .await
            .unwrap();
        storage
            .bank
            .import_transactions(
                id,
                savings_bank,
                vec![
                    bank_transaction("in", day(7), "250"),
                    bank_transaction("in-later", day(8), "250"),
                ],
                None,
            )
            .await
            .unwrap();
        storage
            .bank
            .import_transactions(
                id,
                unlinked_bank,
                vec![bank_transaction("loose", day(7), "250")],
                None,
            )
            .await
            .unwrap();

        let id_of = |pending: &[crate::models::bank_sync::BankTransaction], external_id: &str| {
            pending
                .iter()
                .find(|transaction| transaction.external_id == external_id)
                .unwrap()
                .id
        };
        let checking_pending = storage
            .bank
            .list_pending_transactions(id, checking_bank)
            .await
            .unwrap();
        let out = id_of(&checking_pending, "out");

        let prefill = storage
            .bank
            .find_pending_prefill(id, out)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(prefill.bank_transaction_id, out);
        assert_eq!(prefill.date, day(7));
        assert_eq!(prefill.amount, dec("-250"));
        assert_eq!(prefill.internal_account_id, Some(checking));
        assert_eq!(prefill.internal_account_name.as_deref(), Some("Checking"));
        assert_eq!(prefill.currency, "EUR");

        let for_day = storage
            .bank
            .list_pending_prefills(id, day(7))
            .await
            .unwrap();
        let described: Vec<_> = for_day
            .iter()
            .map(|prefill| (prefill.internal_account_name.as_deref(), prefill.amount))
            .collect();
        // Unlinked first, then by ledger account name, larger amounts first.
        assert_eq!(
            described,
            [
                (None, dec("250")),
                (Some("Checking"), dec("-3")),
                (Some("Checking"), dec("-250")),
                (Some("Savings"), dec("250")),
            ]
        );

        let matches = storage.bank.list_transfer_matches(id, out).await.unwrap();
        let matched: Vec<_> = matches
            .iter()
            .map(|prefill| prefill.internal_account_name.as_deref())
            .collect();
        assert_eq!(matched, [None, Some("Savings")]);
        assert!(matches.iter().all(|prefill| prefill.amount == dec("250")));

        let stranger = user(&storage, "bob").await;
        assert!(storage
            .bank
            .find_pending_prefill(stranger, out)
            .await
            .unwrap()
            .is_none());
        assert!(storage
            .bank
            .list_transfer_matches(stranger, out)
            .await
            .unwrap()
            .is_empty());
    })
    .await;
}

#[tokio::test]
async fn deleting_a_bank_account_removes_its_bank_transactions_and_unlinks_entries() {
    on_each_database(|storage| async move {
        let id = user(&storage, "ada").await;
        let checking = account(&storage, id, "Checking", AccountType::Asset).await;
        let income = account(&storage, id, "Income", AccountType::Revenue).await;
        let (bank_account_id, bank_transaction_id) =
            pending_bank_transaction(&storage, id, Some(checking), "100").await;

        let transaction_id = storage
            .ledger
            .create_transaction(
                id,
                draft(
                    day(5),
                    "Salary",
                    vec![
                        linked_entry(checking, EntryType::Debit, "100", bank_transaction_id),
                        entry(income, EntryType::Credit, "100"),
                    ],
                ),
            )
            .await
            .unwrap();

        assert!(storage
            .bank
            .delete_account(id, bank_account_id)
            .await
            .unwrap());

        let found = storage
            .ledger
            .find_transaction(id, transaction_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.entries.len(), 2);
        assert!(found
            .entries
            .iter()
            .all(|entry| entry.bank_transaction_id.is_none()));
    })
    .await;
}
