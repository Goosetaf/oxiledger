use crate::{
    models::{
        account::Account,
        transaction::{CreateTransactionRequest, EntryType, JournalEntry, Transaction},
    },
    views::{
        transaction_form::{EntryRow, TransactionForm},
        transaction_new::{entry_rows_to_inputs, list_pending_bank_transaction_prefills},
    },
};
use dioxus::prelude::*;
use uuid::Uuid;

#[cfg(feature = "server")]
use {
    crate::models::account::{AccountType, NormalBalance},
    crate::views::transaction_new::validate_linked_bank_transactions,
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
                je.memo,
                je.bank_transaction_id AS "bank_transaction_id?: Uuid"
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
                bank_transaction_id: row.bank_transaction_id,
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

        validate_linked_bank_transactions(&mut tx, &req, user_id, Some(id)).await?;

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

        // Before deleting journal entries, unlink any bank transactions that were
        // previously linked to entries that are NOT being re-submitted (i.e. removed
        // by the user). We do this by resetting bank_transactions to 'pending' for
        // any linked entry whose bank_transaction_id does NOT appear in the new set.
        let new_bank_txn_ids: Vec<uuid::Uuid> = req
            .entries
            .iter()
            .filter_map(|e| e.bank_transaction_id)
            .collect();

        // Reset bank_transactions that are no longer being linked.
        sqlx::query!(
            r#"UPDATE bank_transactions bt
               SET status = 'pending', linked_journal_entry_id = NULL
               FROM journal_entries je
               WHERE je.transaction_id = $1
                 AND je.bank_transaction_id = bt.id
                 AND bt.user_id = $2
                 AND bt.id <> ALL($3)"#,
            id,
            user_id,
            &new_bank_txn_ids
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

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
            let je_id = sqlx::query_scalar!(
                "INSERT INTO journal_entries (transaction_id, account_id, entry_type, amount, memo, bank_transaction_id)
                 VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
                id,
                entry.account_id,
                entry.entry_type as _,
                entry.amount,
                entry
                    .memo
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty()),
                entry.bank_transaction_id
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

            // Re-link the bank transaction if present.
            if let Some(bank_txn_id) = entry.bank_transaction_id {
                sqlx::query!(
                    r#"UPDATE bank_transactions
                       SET status = 'linked', linked_journal_entry_id = $1
                       WHERE id = $2 AND user_id = $3"#,
                    je_id,
                    bank_txn_id,
                    user_id
                )
                .execute(&mut *tx)
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            }
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
    let pending_bank_txns_resource = use_resource(move || async move {
        let date = chrono::NaiveDate::parse_from_str(&txn_date(), "%Y-%m-%d")
            .unwrap_or(transaction.date);
        list_pending_bank_transaction_prefills(date.to_string()).await
    });

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let nav = nav.clone();
        async move {
            submitting.set(true);
            form_error.set(None);

            let entries = entry_rows_to_inputs(&entry_rows());

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
            subtitle: "Adjust the transaction header and entry lines while keeping the journal balanced. Only same-date bank transactions can be linked together."
                .to_string(),
            submit_label: "Save changes".to_string(),
            submitting_label: "Saving changes...".to_string(),
            accounts,
            pending_bank_txns: pending_bank_txns_resource,
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
