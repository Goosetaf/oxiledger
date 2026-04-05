use crate::{
    models::transaction::{CreateTransactionRequest, JournalEntryInput},
    views::{
        transaction_form::{EntryRow, TransactionForm},
        transactions::list_accounts_for_txn,
    },
};
use dioxus::prelude::*;
use uuid::Uuid;

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
            let je_id = sqlx::query_scalar!(
                "INSERT INTO journal_entries (transaction_id, account_id, entry_type, amount, memo, bank_transaction_id)
                 VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
                txn_id,
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

            // If this entry is linked to a bank transaction, mark it as linked
            // and record the journal entry reference.
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

        return Ok(txn_id);
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

// ── Server function: load prefill data for a bank transaction ─────────────────

#[get("/api/bank-sync/transactions/:bank_txn_id/prefill")]
pub async fn get_bank_transaction_prefill(
    bank_txn_id: Uuid,
) -> Result<crate::models::bank_sync::BankTransactionPrefill, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let row = sqlx::query!(
            r#"SELECT bt.id, bt.date, bt.amount, bt.description, bt.reference,
                      ba.internal_account_id, a.name AS internal_account_name,
                      bt.currency
               FROM bank_transactions bt
               JOIN bank_accounts ba ON ba.id = bt.bank_account_id
               JOIN accounts a ON a.id = ba.internal_account_id
               WHERE bt.id = $1 AND bt.user_id = $2 AND bt.status = 'pending'"#,
            bank_txn_id,
            user_id
        )
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| {
            ServerFnError::new("Bank transaction not found or already linked/dismissed")
        })?;

        return Ok(crate::models::bank_sync::BankTransactionPrefill {
            bank_transaction_id: row.id,
            date: row.date,
            amount: row.amount,
            description: row.description,
            reference: row.reference,
            internal_account_id: row.internal_account_id,
            internal_account_name: row.internal_account_name,
            currency: row.currency,
        });
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

// ── Components ────────────────────────────────────────────────────────────────

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
                        bank_transaction_id: row.locked_bank_transaction_id,
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

/// New transaction pre-filled from a bank transaction.
/// The bank-side entry row is locked and auto-populated.
#[component]
pub fn NewTransactionFromBank(bank_txn_id: Uuid) -> Element {
    let nav = use_navigator();
    let accounts = use_loader(list_accounts_for_txn)?.read().clone();

    // Load the prefill data from the server.
    let prefill_resource = use_server_future(move || get_bank_transaction_prefill(bank_txn_id))?;

    let prefill = match prefill_resource() {
        Some(Ok(p)) => p,
        Some(Err(err)) => {
            return rsx! {
                div { class: "app-container",
                    div { class: "page-stack",
                        div { class: "message message-error", "Failed to load bank transaction: {err}" }
                    }
                }
            };
        }
        None => {
            return rsx! {
                div { class: "app-container",
                    div { class: "page-stack",
                        div { class: "message message-info", "Loading bank transaction..." }
                    }
                }
            };
        }
    };

    // Determine the locked bank entry row direction:
    // positive amount → inflow → credit to bank account (credit row)
    // negative amount → outflow → debit from bank account (debit row)
    let (debit_str, credit_str) = if prefill.amount >= rust_decimal::Decimal::ZERO {
        (String::new(), prefill.amount.abs().to_string())
    } else {
        (prefill.amount.abs().to_string(), String::new())
    };

    let locked_row = EntryRow {
        account_id_str: prefill.internal_account_id.to_string(),
        memo: prefill.description.clone(),
        debit_amount_str: debit_str,
        credit_amount_str: credit_str,
        locked_bank_transaction_id: Some(prefill.bank_transaction_id),
    };

    let txn_date = use_signal(|| prefill.date.format("%Y-%m-%d").to_string());
    let txn_desc = use_signal(|| prefill.description.clone());
    let txn_ref = use_signal(|| prefill.reference.clone().unwrap_or_default());
    let entry_rows: Signal<Vec<EntryRow>> = use_signal(|| vec![locked_row, EntryRow::new()]);
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
                        bank_transaction_id: row.locked_bank_transaction_id,
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
            heading: "New transaction from bank".to_string(),
            subtitle: "The highlighted row is linked to a bank transaction and is read-only.".to_string(),
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
