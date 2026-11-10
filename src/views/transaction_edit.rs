use crate::{
    models::{
        account::Account,
        transaction::{CreateTransactionRequest, Transaction},
    },
    views::{
        transaction_form::{EntryRow, TransactionForm},
        transaction_new::{entry_rows_to_inputs, list_pending_bank_transaction_prefills},
    },
};
use dioxus::prelude::*;
use uuid::Uuid;

#[get("/api/transactions/accounts/all")]
async fn list_accounts_for_edit() -> Result<Vec<Account>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let mut accounts = storage.accounts.list(user_id).await?;
        // Active accounts first, each group keeping the list's own order.
        accounts.sort_by_key(|account| !account.is_active);

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

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return storage
            .transactions
            .find(user_id, id)
            .await?
            .ok_or_else(|| ServerFnError::new("Transaction not found"));
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[post("/api/transactions/:id")]
async fn update_transaction(id: Uuid, req: CreateTransactionRequest) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::{
            server::auth::{extract_context, require_auth},
            views::transaction_new::{transaction_draft, transaction_save_error},
        };

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let draft = transaction_draft(&storage, req, user_id, Some(id)).await?;

        let updated = storage
            .transactions
            .update(user_id, id, draft)
            .await
            .map_err(transaction_save_error)?;

        if !updated {
            return Err(ServerFnError::new("Transaction not found"));
        }
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
        let date =
            chrono::NaiveDate::parse_from_str(&txn_date(), "%Y-%m-%d").unwrap_or(transaction.date);
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
