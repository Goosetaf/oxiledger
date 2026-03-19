use crate::{
    models::transaction::{CreateTransactionRequest, JournalEntryInput},
    views::{
        transaction_form::{EntryRow, TransactionForm},
        transactions::list_accounts_for_txn,
    },
};
use dioxus::prelude::*;
use uuid::Uuid;

#[cfg(feature = "server")]
use {sqlx::PgPool, tower_cookies::Cookies};

#[post("/api/transactions")]
pub async fn create_transaction(req: CreateTransactionRequest) -> Result<Uuid, ServerFnError> {
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

        let txn_id = sqlx::query_scalar!(
            "INSERT INTO transactions (user_id, date, description, reference)
             VALUES ($1, $2, $3, $4) RETURNING id",
            user_id,
            req.date,
            req.description.trim(),
            req.reference
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        for entry in &req.entries {
            sqlx::query!(
                "INSERT INTO journal_entries (transaction_id, account_id, entry_type, amount, memo)
                 VALUES ($1, $2, $3, $4, $5)",
                txn_id,
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

        return Ok(txn_id);
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[component]
pub fn NewTransaction() -> Element {
    let nav = use_navigator();
    let accounts = use_loader(list_accounts_for_txn)?.read().clone();

    let txn_date = use_signal(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let txn_desc = use_signal(String::new);
    let txn_ref = use_signal(String::new);
    let entry_rows: Signal<Vec<EntryRow>> = use_signal(|| vec![EntryRow::new(), EntryRow::new()]);
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
                .unwrap_or_else(|_| chrono::Local::now().date_naive());

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

            match create_transaction(req).await {
                Ok(_) => {
                    let _ = nav.push(crate::Route::Transactions {});
                }
                Err(e) => form_error.set(Some(e.to_string())),
            }

            submitting.set(false);
        }
    };

    rsx! {
        TransactionForm {
            heading: "New transaction".to_string(),
            subtitle: "Build a balanced entry with two or more lines before saving.".to_string(),
            submit_label: "Save transaction".to_string(),
            submitting_label: "Saving transaction...".to_string(),
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
