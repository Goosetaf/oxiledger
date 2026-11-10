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
use crate::storage::{transactions::TransactionDraft, Storage, StorageError};

/// Checks what the transaction form sent and tidies it for storing. `current_transaction_id`
/// is the transaction being edited, whose own bank transactions stay available to it.
#[cfg(feature = "server")]
pub(crate) async fn transaction_draft(
    storage: &Storage,
    req: CreateTransactionRequest,
    user_id: Uuid,
    current_transaction_id: Option<Uuid>,
) -> Result<TransactionDraft, ServerFnError> {
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

    validate_linked_bank_transactions(storage, &req, user_id, current_transaction_id).await?;

    let non_empty = |value: Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    Ok(TransactionDraft {
        date: req.date,
        description: req.description.trim().to_string(),
        reference: non_empty(req.reference),
        entries: req
            .entries
            .into_iter()
            .map(|entry| JournalEntryInput {
                memo: non_empty(entry.memo),
                ..entry
            })
            .collect(),
    })
}

#[cfg(feature = "server")]
pub(crate) fn transaction_save_error(err: StorageError) -> ServerFnError {
    match err {
        // Another request linked the bank transaction after it was checked.
        StorageError::Conflict(_) => {
            ServerFnError::new("One or more bank transactions are already linked elsewhere")
        }
        other => other.into(),
    }
}

#[cfg(feature = "server")]
async fn validate_linked_bank_transactions(
    storage: &Storage,
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

    let expected_count = bank_txn_ids.len();
    let rows = storage
        .bank_transactions
        .link_info(user_id, bank_txn_ids)
        .await?;

    if rows.len() != expected_count {
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
                .is_some_and(|txn_id| row.linked_transaction_id == Some(txn_id));

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

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let draft = transaction_draft(&storage, req, user_id, None).await?;

        return storage
            .transactions
            .create(user_id, draft)
            .await
            .map_err(transaction_save_error);
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

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return storage
            .bank_transactions
            .find_pending_prefill(user_id, bank_txn_id)
            .await?
            .ok_or_else(|| {
                ServerFnError::new("Bank transaction not found or already linked/dismissed")
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

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return Ok(storage
            .bank_transactions
            .list_pending_prefills(user_id, date)
            .await?);
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

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return Ok(storage
            .bank_transactions
            .list_transfer_matches(user_id, bank_txn_id)
            .await?);
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
                div { class: "app-container message message-info",
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
