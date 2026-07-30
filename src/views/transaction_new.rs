use crate::{
    components::button::Button,
    models::bank_sync::BankTransactionPrefill,
    models::transaction::{CreateTransactionRequest, JournalEntryInput},
    views::{
        transaction_form::{EntryRow, TransactionForm},
        transactions::list_accounts_for_txn,
    },
};
use dioxus::prelude::*;
use uuid::Uuid;

#[cfg(feature = "server")]
pub(crate) async fn validate_linked_bank_transactions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    req: &CreateTransactionRequest,
    user_id: Uuid,
    current_transaction_id: Option<Uuid>,
) -> Result<(), ServerFnError> {
    use crate::models::transaction::EntryType;
    use rust_decimal::Decimal;
    use std::collections::{HashMap, HashSet};

    let mut entry_by_bank_txn = HashMap::new();
    let mut bank_txn_ids = Vec::new();
    let mut seen_ids = HashSet::new();

    for entry in &req.entries {
        if let Some(bank_txn_id) = entry.bank_transaction_id {
            if !seen_ids.insert(bank_txn_id) {
                return Err(ServerFnError::new(
                    "The same bank transaction cannot be linked more than once",
                ));
            }
            bank_txn_ids.push(bank_txn_id);
            entry_by_bank_txn.insert(bank_txn_id, entry);
        }
    }

    if bank_txn_ids.is_empty() {
        return Ok(());
    }

    let rows = sqlx::query!(
        r#"
        SELECT
            bt.id,
            bt.date,
            bt.amount AS "amount: Decimal",
            bt.status,
        ba.internal_account_id AS "internal_account_id?: Uuid",
            linked_je.transaction_id AS "existing_transaction_id?: Uuid"
        FROM bank_transactions bt
        JOIN bank_accounts ba ON ba.id = bt.bank_account_id
        LEFT JOIN journal_entries linked_je ON linked_je.id = bt.linked_journal_entry_id
        WHERE bt.id = ANY($1) AND bt.user_id = $2
        "#,
        &bank_txn_ids,
        user_id
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| ServerFnError::new(e.to_string()))?;

    if rows.len() != bank_txn_ids.len() {
        return Err(ServerFnError::new(
            "One or more linked bank transactions could not be found",
        ));
    }

    for row in rows {
        let Some(entry) = entry_by_bank_txn.get(&row.id) else {
            continue;
        };

        if row.date != req.date {
            return Err(ServerFnError::new(
                "Linked bank transactions must have the same date as the journal transaction",
            ));
        }

        if let Some(expected_id) = row.internal_account_id {
            if entry.account_id != expected_id {
                return Err(ServerFnError::new(
                    "A linked bank transaction must use its mapped internal account",
                ));
            }
        }

        let expected_entry_type = if row.amount >= Decimal::ZERO {
            EntryType::Debit
        } else {
            EntryType::Credit
        };
        let expected_amount = row.amount.abs();

        if entry.entry_type != expected_entry_type || entry.amount != expected_amount {
            return Err(ServerFnError::new(
                "A linked bank transaction line must match the imported amount and direction",
            ));
        }

        let is_available = row.status == "pending"
            || current_transaction_id
                .is_some_and(|txn_id| row.existing_transaction_id == Some(txn_id));

        if !is_available {
            return Err(ServerFnError::new(
                "One or more bank transactions are already linked elsewhere",
            ));
        }
    }

    Ok(())
}

pub(crate) fn entry_rows_to_inputs(rows: &[EntryRow]) -> Vec<JournalEntryInput> {
    rows.iter()
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
        .collect()
}

fn pending_bank_transactions_for_date(
    date_text: &str,
) -> impl std::future::Future<Output = Result<Vec<BankTransactionPrefill>, ServerFnError>> + 'static
{
    let date_text = date_text.to_string();
    async move { list_pending_bank_transaction_prefills(date_text).await }
}

fn transfer_description(from_account: &str, to_account: &str) -> String {
    format!("Transfer: {} -> {}", from_account, to_account)
}

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

        validate_linked_bank_transactions(&mut tx, &req, user_id, None).await?;

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
) -> Result<BankTransactionPrefill, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let row = sqlx::query!(
            r#"SELECT bt.id, bt.date, bt.amount, bt.description, bt.reference,
                      ba.internal_account_id AS "internal_account_id?: Uuid",
                      a.name AS "internal_account_name?",
                      bt.currency
               FROM bank_transactions bt
               JOIN bank_accounts ba ON ba.id = bt.bank_account_id
               LEFT JOIN accounts a ON a.id = ba.internal_account_id
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

        return Ok(BankTransactionPrefill {
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

#[get("/api/bank-sync/transactions/pending/:date_text")]
pub async fn list_pending_bank_transaction_prefills(
    date_text: String,
) -> Result<Vec<BankTransactionPrefill>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let date = chrono::NaiveDate::parse_from_str(&date_text, "%Y-%m-%d")
            .map_err(|_| ServerFnError::new("Invalid transaction date"))?;

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let rows = sqlx::query!(
            r#"SELECT bt.id, bt.date, bt.amount, bt.description, bt.reference,
                      ba.internal_account_id AS "internal_account_id?: Uuid",
                      a.name AS "internal_account_name?",
                      bt.currency
               FROM bank_transactions bt
               JOIN bank_accounts ba ON ba.id = bt.bank_account_id
               LEFT JOIN accounts a ON a.id = ba.internal_account_id
               WHERE bt.user_id = $1 AND bt.status = 'pending' AND bt.date = $2
               ORDER BY COALESCE(a.code, ''), COALESCE(a.name, ''), bt.amount DESC, bt.imported_at DESC"#,
            user_id,
            date
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(rows
            .into_iter()
            .map(|row| BankTransactionPrefill {
                bank_transaction_id: row.id,
                date: row.date,
                amount: row.amount,
                description: row.description,
                reference: row.reference,
                internal_account_id: row.internal_account_id,
                internal_account_name: row.internal_account_name,
                currency: row.currency,
            })
            .collect());
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[get("/api/bank-sync/transactions/:bank_txn_id/transfer-suggestions")]
pub async fn suggest_transfer_matches(
    bank_txn_id: Uuid,
) -> Result<Vec<BankTransactionPrefill>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let rows = sqlx::query!(
            r#"
            SELECT
                bt.id,
                bt.date,
                bt.amount,
                bt.description,
                bt.reference,
                ba.internal_account_id AS "internal_account_id?: Uuid",
                a.name AS "internal_account_name?",
                bt.currency
            FROM bank_transactions source
            JOIN bank_transactions bt
                ON bt.user_id = source.user_id
               AND bt.status = 'pending'
               AND bt.id <> source.id
               AND bt.bank_account_id <> source.bank_account_id
               AND bt.date = source.date
               AND bt.amount = -source.amount
            JOIN bank_accounts ba ON ba.id = bt.bank_account_id
            LEFT JOIN accounts a ON a.id = ba.internal_account_id
            WHERE source.id = $1 AND source.user_id = $2 AND source.status = 'pending'
            ORDER BY COALESCE(a.code, ''), COALESCE(a.name, ''), bt.imported_at DESC
            "#,
            bank_txn_id,
            user_id
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(rows
            .into_iter()
            .map(|row| BankTransactionPrefill {
                bank_transaction_id: row.id,
                date: row.date,
                amount: row.amount,
                description: row.description,
                reference: row.reference,
                internal_account_id: row.internal_account_id,
                internal_account_name: row.internal_account_name,
                currency: row.currency,
            })
            .collect());
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
    let pending_bank_txns_resource =
        use_resource(move || pending_bank_transactions_for_date(&txn_date()));

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let nav = nav.clone();
        async move {
            submitting.set(true);
            form_error.set(None);

            let entries = entry_rows_to_inputs(&entry_rows());

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
            subtitle: "Build a balanced entry with two or more lines before saving. Only same-date bank transactions can be linked together."
                .to_string(),
            submit_label: "Save transaction".to_string(),
            submitting_label: "Saving transaction...".to_string(),
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
    // positive amount → inflow → debit to bank account (debit row)
    // negative amount → outflow → credit from bank account (credit row)
    let (debit_str, credit_str) = if prefill.amount >= rust_decimal::Decimal::ZERO {
        (prefill.amount.abs().to_string(), String::new())
    } else {
        (String::new(), prefill.amount.abs().to_string())
    };

    let locked_row = EntryRow {
        account_id_str: prefill
            .internal_account_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        memo: prefill.description.clone(),
        debit_amount_str: debit_str,
        credit_amount_str: credit_str,
        locked_bank_transaction_id: Some(prefill.bank_transaction_id),
    };

    let txn_date = use_signal(|| prefill.date.format("%Y-%m-%d").to_string());
    let mut txn_desc = use_signal(|| prefill.description.clone());
    let txn_ref = use_signal(|| prefill.reference.clone().unwrap_or_default());
    let mut entry_rows: Signal<Vec<EntryRow>> = use_signal(|| vec![locked_row, EntryRow::new()]);
    let mut form_error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);
    let pending_bank_txns_resource =
        use_resource(move || pending_bank_transactions_for_date(&txn_date()));
    let transfer_suggestions_resource = use_resource(move || suggest_transfer_matches(bank_txn_id));
    let source_account_name = prefill.internal_account_name.clone().unwrap_or_default();
    let selected_bank_txn_ids: Vec<Uuid> = entry_rows()
        .iter()
        .filter_map(|row| row.locked_bank_transaction_id)
        .collect();
    let transfer_suggestions: Vec<_> = transfer_suggestions_resource()
        .and_then(Result::ok)
        .unwrap_or_default()
        .into_iter()
        .filter(|suggestion| !selected_bank_txn_ids.contains(&suggestion.bank_transaction_id))
        .collect();

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let nav = nav.clone();
        async move {
            submitting.set(true);
            form_error.set(None);

            let entries = entry_rows_to_inputs(&entry_rows());

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
        div { class: "page-stack",
            for suggestion in transfer_suggestions {
                div { class: "message message-info",
                    div { class: "actions-row",
                        div { class: "stack-sm",
                            strong { "Possible internal transfer" }
                            span { class: "supporting-text",
                                "Match {suggestion.internal_account_name.as_deref().unwrap_or(\"Unknown account\")} {suggestion.amount} {suggestion.currency} on {suggestion.date}"
                            }
                        }
                        Button {
                            class: "btn btn-secondary btn-sm".to_string(),
                            onclick: {
                                let source_account_name = source_account_name.clone();
                                let suggestion = suggestion.clone();
                                move |_| {
                                    let (debit_amount_str, credit_amount_str) = if suggestion.amount
                                        >= rust_decimal::Decimal::ZERO
                                    {
                                        (suggestion.amount.abs().to_string(), String::new())
                                    } else {
                                        (String::new(), suggestion.amount.abs().to_string())
                                    };
                                    let mut rows = entry_rows.write();
                                    if let Some(blank_row) = rows
                                        .iter_mut()
                                        .find(|row| {
                                            !row.is_locked() && row.account_id_str.is_empty()
                                                && row.memo.trim().is_empty()
                                                && row.debit_amount_str.trim().is_empty()
                                                && row.credit_amount_str.trim().is_empty()
                                        })
                                    {
                                        blank_row.account_id_str = suggestion
                                            .internal_account_id
                                            .map(|id| id.to_string())
                                            .unwrap_or_default();
                                        blank_row.memo = suggestion.description.clone();
                                        blank_row.debit_amount_str = debit_amount_str;
                                        blank_row.credit_amount_str = credit_amount_str;
                                        blank_row.locked_bank_transaction_id = Some(
                                            suggestion.bank_transaction_id,
                                        );
                                    } else {
                                        rows.push(EntryRow {
                                            account_id_str: suggestion
                                                .internal_account_id
                                                .map(|id| id.to_string())
                                                .unwrap_or_default(),
                                            memo: suggestion.description.clone(),
                                            debit_amount_str,
                                            credit_amount_str,
                                            locked_bank_transaction_id: Some(suggestion.bank_transaction_id),
                                        });
                                    }
                                    txn_desc
                                        .set(
                                            transfer_description(
                                                &source_account_name,
                                                suggestion
                                                    .internal_account_name
                                                    .as_deref()
                                                    .unwrap_or("Unknown account"),
                                            ),
                                        );
                                }
                            },
                            "Match as transfer"
                        }
                    }
                }
            }

            TransactionForm {
                heading: "New transaction from bank".to_string(),
                subtitle: "The highlighted row is linked to a bank transaction and is read-only. Only same-date bank transactions can be linked together."
                    .to_string(),
                submit_label: "Save transaction".to_string(),
                submitting_label: "Saving transaction...".to_string(),
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
}
