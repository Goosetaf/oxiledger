use crate::{
    models::{
        account::Account,
        transaction::{
            CreateTransactionRequest, EntryType, JournalEntry, JournalEntryInput, Transaction,
        },
    },
    views::transaction_form::{EntryRow, TransactionForm},
};
use dioxus::prelude::*;
use uuid::Uuid;

#[cfg(feature = "server")]
use {
    crate::models::account::{AccountType, NormalBalance},
    rust_decimal::Decimal,
};

#[get("/api/transactions/accounts/all")]
async fn list_accounts_for_edit() -> Result<Vec<Account>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let accounts = sqlx::query_as!(
            Account,
            r#"SELECT id, user_id, name, code,
                account_type AS "account_type: AccountType",
                normal_balance AS "normal_balance: NormalBalance",
                description, is_active
            FROM accounts
            WHERE user_id = $1
            ORDER BY is_active DESC, account_type, COALESCE(code, ''), name"#,
            user_id
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(accounts);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[get("/api/transactions/:id")]
async fn get_transaction(id: Uuid) -> Result<Transaction, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let rows = sqlx::query!(
            r#"
            SELECT
                t.id AS "transaction_id: Uuid",
                t.date,
                t.description,
                t.reference,
                t.user_id,
                je.id AS "entry_id: Uuid",
                je.account_id AS "account_id: Uuid",
                a.name AS "account_name?",
                a.code AS "account_code?",
                je.entry_type AS "entry_type: EntryType",
                je.amount AS "amount: Decimal",
                je.memo
            FROM transactions t
            JOIN journal_entries je ON je.transaction_id = t.id
            JOIN accounts a ON a.id = je.account_id
            WHERE t.id = $1 AND t.user_id = $2
            ORDER BY je.created_at, je.id
            "#,
            id,
            user_id
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        let first_row = rows
            .first()
            .ok_or_else(|| ServerFnError::new("Transaction not found"))?;

        let entries = rows
            .iter()
            .map(|row| JournalEntry {
                id: row.entry_id,
                transaction_id: row.transaction_id,
                account_id: row.account_id,
                account_name: row.account_name.clone(),
                account_code: row.account_code.clone(),
                entry_type: row.entry_type,
                amount: row.amount,
                memo: row.memo.clone(),
            })
            .collect();

        return Ok(Transaction {
            id: first_row.transaction_id,
            user_id: first_row.user_id,
            date: first_row.date,
            description: first_row.description.clone(),
            reference: first_row.reference.clone(),
            entries,
        });
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[post("/api/transactions/:id")]
async fn update_transaction(id: Uuid, req: CreateTransactionRequest) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        if req.entries.len() < 2 {
            return Err(ServerFnError::new(
                "A transaction requires at least two journal entry lines",
            ));
        }

        if !req.is_balanced() {
            return Err(ServerFnError::new(
                "Transaction is not balanced: sum of debits must equal sum of credits",
            ));
        }

        let mut tx = pool
            .begin()
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        let updated = sqlx::query!(
            r#"UPDATE transactions
            SET date = $3,
                description = $4,
                reference = $5,
                updated_at = NOW()
            WHERE id = $1 AND user_id = $2"#,
            id,
            user_id,
            req.date,
            req.description.trim(),
            req.reference
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        if updated.rows_affected() == 0 {
            return Err(ServerFnError::new("Transaction not found"));
        }

        sqlx::query!(
            r#"DELETE FROM journal_entries
            USING transactions
            WHERE journal_entries.transaction_id = transactions.id
              AND transactions.id = $1
              AND transactions.user_id = $2"#,
            id,
            user_id
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        for entry in &req.entries {
            sqlx::query!(
                "INSERT INTO journal_entries (transaction_id, account_id, entry_type, amount, memo)
                 VALUES ($1, $2, $3, $4, $5)",
                id,
                entry.account_id,
                entry.entry_type as _,
                entry.amount,
                entry
                    .memo
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
    }

    Ok(())
}

#[component]
pub fn EditTransaction(id: Uuid) -> Element {
    let nav = use_navigator();
    let transaction_resource = use_server_future(move || get_transaction(id))?;
    let accounts = use_loader(list_accounts_for_edit)?.read().clone();

    let transaction = match transaction_resource() {
        Some(Ok(transaction)) => transaction,
        Some(Err(err)) => {
            return rsx! {
                div { class: "app-container",
                    div { class: "page-stack",
                        div { class: "message message-error", "{err}" }
                    }
                }
            };
        }
        None => {
            return rsx! {
                div { class: "app-container",
                    div { class: "page-stack",
                        div { class: "message message-info", "Loading transaction..." }
                    }
                }
            };
        }
    };

    let txn_date = use_signal(|| transaction.date.format("%Y-%m-%d").to_string());
    let txn_desc = use_signal(|| transaction.description.clone());
    let txn_ref = use_signal(|| transaction.reference.clone().unwrap_or_default());
    let entry_rows: Signal<Vec<EntryRow>> = use_signal(|| {
        transaction
            .entries
            .iter()
            .map(EntryRow::from_journal_entry)
            .collect()
    });
    let mut form_error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let nav = nav.clone();
        async move {
            submitting.set(true);
            form_error.set(None);

            let entries: Vec<JournalEntryInput> = entry_rows()
                .iter()
                .filter_map(|row| {
                    let account_id = Uuid::parse_str(&row.account_id_str).ok()?;
                    let (entry_type, amount) = row.journal_entry()?;
                    Some(JournalEntryInput {
                        account_id,
                        entry_type,
                        amount,
                        memo: if row.memo.trim().is_empty() {
                            None
                        } else {
                            Some(row.memo.clone())
                        },
                    })
                })
                .collect();

            let date = chrono::NaiveDate::parse_from_str(&txn_date(), "%Y-%m-%d")
                .unwrap_or(transaction.date);

            let req = CreateTransactionRequest {
                date,
                description: txn_desc(),
                reference: if txn_ref().trim().is_empty() {
                    None
                } else {
                    Some(txn_ref())
                },
                entries,
            };

            match update_transaction(id, req).await {
                Ok(_) => {
                    let _ = nav.push(crate::Route::Transactions {});
                }
                Err(err) => form_error.set(Some(err.to_string())),
            }

            submitting.set(false);
        }
    };

    rsx! {
        TransactionForm {
            heading: "Edit transaction".to_string(),
            subtitle: "Adjust the transaction header and entry lines while keeping the journal balanced."
                .to_string(),
            submit_label: "Save changes".to_string(),
            submitting_label: "Saving changes...".to_string(),
            accounts,
            txn_date,
            txn_desc,
            txn_ref,
            entry_rows,
            form_error,
            submitting,
            onsubmit: handle_submit,
        }
    }
}
