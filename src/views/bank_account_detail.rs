use crate::{
    components::button::Button,
    models::bank_sync::{
        BankAccountBalanceComparison, BankAccountSummary, BankTransaction, BankTransactionOverview,
        SyncResult,
    },
};
use dioxus::prelude::*;
use rust_decimal::Decimal;
use uuid::Uuid;

#[get("/api/bank-accounts/:id")]
pub async fn get_bank_account_detail(id: Uuid) -> Result<BankAccountSummary, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return storage
            .bank_accounts
            .find_summary(user_id, id)
            .await?
            .ok_or_else(|| ServerFnError::new("Bank account not found"));
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[get("/api/bank-accounts/:id/balance")]
pub async fn get_bank_account_balance(
    id: Uuid,
) -> Result<BankAccountBalanceComparison, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        crate::server::bank_sync_cache::maybe_lazy_sync_for_user(&storage, user_id, id)
            .await
            .map_err(ServerFnError::new)?;

        let account = storage
            .bank_accounts
            .find(user_id, id)
            .await?
            .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

        // Zero if no ledger account is linked.
        let internal_balance = match account.internal_account_id {
            Some(account_id) => storage.accounts.balance(user_id, account_id).await?,
            None => Decimal::ZERO,
        };

        // Manual accounts have no live balance from a provider.
        if account.is_manual {
            return Ok(BankAccountBalanceComparison {
                bank_account_id: id,
                internal_balance,
                bank_balance: None,
                bank_balance_currency: None,
                difference: None,
            });
        }

        let bank_balance_currency = account.balance.map(|_| account.currency);

        // Only show a difference when we have both sides.
        let difference = match (account.balance, account.internal_account_id) {
            (Some(bank), Some(_)) => Some(bank - internal_balance),
            _ => None,
        };

        return Ok(BankAccountBalanceComparison {
            bank_account_id: id,
            internal_balance,
            bank_balance: account.balance,
            bank_balance_currency,
            difference,
        });
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[get("/api/bank-accounts/:id/transactions")]
pub async fn list_bank_transactions(id: Uuid) -> Result<Vec<BankTransaction>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        crate::server::bank_sync_cache::maybe_lazy_sync_for_user(&storage, user_id, id)
            .await
            .map_err(ServerFnError::new)?;

        return Ok(storage.bank_transactions.list_pending(user_id, id).await?);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[get("/api/bank-accounts/:id/transactions/all")]
pub async fn list_all_bank_transactions(
    id: Uuid,
) -> Result<Vec<BankTransactionOverview>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        crate::server::bank_sync_cache::maybe_lazy_sync_for_user(&storage, user_id, id)
            .await
            .map_err(ServerFnError::new)?;

        return Ok(storage.bank_transactions.list_overview(user_id, id).await?);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[post("/api/bank-transactions/:id/restore")]
pub async fn restore_bank_transaction(id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        if !storage.bank_transactions.restore(user_id, id).await? {
            return Err(ServerFnError::new(
                "Transaction not found or not in dismissed state",
            ));
        }
    }

    Ok(())
}

#[post("/api/bank-accounts/:id/transactions")]
pub async fn create_manual_bank_transaction(
    id: Uuid,
    date: chrono::NaiveDate,
    description: String,
    amount: rust_decimal::Decimal,
    reference: Option<String>,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::{
            server::auth::{extract_context, require_auth},
            storage::bank_transactions::NewBankTransaction,
        };

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        storage
            .bank_accounts
            .find(user_id, id)
            .await?
            .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

        storage
            .bank_transactions
            .create(
                user_id,
                id,
                NewBankTransaction {
                    external_id: format!("manual-{}", Uuid::new_v4()),
                    date,
                    amount,
                    currency: "EUR".to_string(),
                    description,
                    reference,
                },
            )
            .await?;
    }

    Ok(())
}

#[post("/api/bank-transactions/:id/dismiss")]
pub async fn dismiss_bank_transaction(id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        if !storage.bank_transactions.dismiss(user_id, id).await? {
            return Err(ServerFnError::new(
                "Transaction not found or already processed",
            ));
        }
    }

    Ok(())
}

#[post("/api/bank-accounts/:id/sync")]
pub async fn sync_bank_account_detail(
    id: Uuid,
    force: Option<bool>,
) -> Result<SyncResult, ServerFnError> {
    // Delegate to the same logic used by the list page.
    crate::views::bank_accounts::sync_bank_account(id, force).await
}

#[post("/api/bank-accounts/:id/disconnect")]
pub async fn disconnect_bank_account(id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let account = storage
            .bank_accounts
            .find_syncable(id, Some(user_id))
            .await?
            .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

        if account.is_manual {
            return Err(ServerFnError::new("This bank account is already manual"));
        }

        storage
            .bank_accounts
            .disconnect_from_provider(user_id, id)
            .await?;

        if let Some(session) = account.session {
            let remaining = storage
                .bank_accounts
                .count_for_connection(session.connection_id)
                .await?;

            if remaining == 0 {
                if let Ok(provider) = crate::bank_sync::get_provider(&session.provider_id).await {
                    let _ = provider.revoke_session(&session.provider_session_id).await;
                }

                storage
                    .bank_connections
                    .delete(user_id, session.connection_id)
                    .await?;
            }
        }
    }

    Ok(())
}

// ── UI ─────────────────────────────────────────────────────────────────────────

#[component]
pub fn BankAccountDetail(id: Uuid) -> Element {
    let nav = use_navigator();
    let account_resource = use_server_future(move || get_bank_account_detail(id))?;
    let mut balance_resource = use_resource(move || get_bank_account_balance(id));
    let mut txns_resource = use_resource(move || list_all_bank_transactions(id));

    let mut status_filter = use_signal(|| "pending");

    let mut action_error = use_signal(|| None::<String>);
    let mut action_success = use_signal(|| None::<String>);
    let mut syncing = use_signal(|| false);
    let mut dismissing = use_signal(|| None::<Uuid>);
    let mut confirm_dismiss = use_signal(|| None::<Uuid>);
    let mut restoring = use_signal(|| None::<Uuid>);
    let mut disconnecting = use_signal(|| false);
    let mut confirm_disconnect = use_signal(|| false);
    let mut show_add_form = use_signal(|| false);
    let mut new_date = use_signal(|| chrono::Local::now().naive_local().date());
    let mut new_description = use_signal(String::new);
    let mut new_amount = use_signal(String::new);
    let mut new_reference = use_signal(String::new);
    let mut adding_txn = use_signal(|| false);
    let mut add_txn_error = use_signal(|| None::<String>);

    let account = match account_resource() {
        Some(Ok(a)) => a,
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
                        div { class: "message message-info", "Loading..." }
                    }
                }
            };
        }
    };

    let handle_sync = move |_| async move {
        action_error.set(None);
        action_success.set(None);
        syncing.set(true);
        match sync_bank_account_detail(id, Some(true)).await {
            Ok(result) => {
                action_success.set(Some(format!(
                    "Sync complete: {} new transaction(s), {} already imported.",
                    result.new_count, result.skipped_count
                )));
                txns_resource.restart();
                balance_resource.restart();
            }
            Err(err) => action_error.set(Some(err.to_string())),
        }
        syncing.set(false);
    };

    let handle_dismiss = move |txn_id: Uuid| async move {
        action_error.set(None);
        dismissing.set(Some(txn_id));
        match dismiss_bank_transaction(txn_id).await {
            Ok(_) => txns_resource.restart(),
            Err(err) => action_error.set(Some(err.to_string())),
        }
        dismissing.set(None);
        confirm_dismiss.set(None);
    };

    let handle_restore = move |txn_id: Uuid| async move {
        action_error.set(None);
        restoring.set(Some(txn_id));
        match restore_bank_transaction(txn_id).await {
            Ok(_) => txns_resource.restart(),
            Err(err) => action_error.set(Some(err.to_string())),
        }
        restoring.set(None);
    };

    let handle_disconnect = move |_| async move {
        action_error.set(None);
        action_success.set(None);
        disconnecting.set(true);
        match disconnect_bank_account(id).await {
            Ok(_) => {
                action_success.set(Some(
                    "Bank connection disconnected. The account is now manual.".to_string(),
                ));
                let _ = nav.push(crate::Route::BankAccounts {});
            }
            Err(err) => {
                action_error.set(Some(err.to_string()));
                disconnecting.set(false);
                confirm_disconnect.set(false);
            }
        }
    };

    let handle_add_txn = move |_| async move {
        add_txn_error.set(None);
        let desc = new_description();
        let desc = desc.trim().to_string();
        if desc.is_empty() {
            add_txn_error.set(Some("Description is required.".to_string()));
            return;
        }
        let amount_str = new_amount();
        let amount: rust_decimal::Decimal = match amount_str.trim().parse() {
            Ok(v) => v,
            Err(_) => {
                add_txn_error.set(Some("Invalid amount.".to_string()));
                return;
            }
        };
        adding_txn.set(true);
        match create_manual_bank_transaction(
            id,
            new_date(),
            desc,
            amount,
            Some(new_reference())
                .filter(|r| !r.trim().is_empty())
                .map(|r| r.trim().to_string()),
        )
        .await
        {
            Ok(_) => {
                show_add_form.set(false);
                new_description.set(String::new());
                new_amount.set(String::new());
                new_reference.set(String::new());
                new_date.set(chrono::Local::now().naive_local().date());
                txns_resource.restart();
            }
            Err(err) => add_txn_error.set(Some(err.to_string())),
        }
        adding_txn.set(false);
    };

    let all_txns: Vec<BankTransactionOverview> =
        txns_resource().and_then(Result::ok).unwrap_or_default();

    let filter = status_filter();
    let filtered_txns: Vec<_> = all_txns
        .iter()
        .filter(|txn| txn.status == filter)
        .cloned()
        .collect();

    let pending_count = all_txns.iter().filter(|t| t.status == "pending").count();
    let linked_count = all_txns.iter().filter(|t| t.status == "linked").count();
    let dismissed_count = all_txns.iter().filter(|t| t.status == "dismissed").count();

    let account_name = account.display_name().to_string();
    let is_manual = account.is_manual;

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h1 { class: "section-title", "{account_name}" }
                        p { class: "section-subtitle",
                            {
                                if is_manual {
                                    format!("Manual · {}", account.currency)
                                } else {
                                    format!(
                                        "{} · {}",
                                        account.aspsp_name.as_deref().unwrap_or("Unknown bank"),
                                        account.currency,
                                    )
                                }
                            }
                        }
                        if let Some(iban) = account.iban.as_deref() {
                            p { class: "section-subtitle mono muted", "{iban}" }
                        }
                    }
                    div { class: "actions-row",
                        Link {
                            class: "btn btn-secondary",
                            to: crate::Route::BankAccountEdit {
                                id,
                            },
                            "Edit"
                        }
                        if is_manual {
                            Link {
                                class: "btn btn-secondary",
                                to: crate::Route::BankConnect {
                                    source_id: Some(id),
                                },
                                "Connect to bank"
                            }
                        } else {
                            Button {
                                class: "btn btn-secondary".to_string(),
                                disabled: syncing(),
                                onclick: handle_sync,
                                if syncing() {
                                    "Syncing..."
                                } else {
                                    "Sync now"
                                }
                            }
                            if confirm_disconnect() {
                                Button {
                                    class: "btn btn-danger".to_string(),
                                    disabled: disconnecting(),
                                    onclick: handle_disconnect,
                                    if disconnecting() {
                                        "Disconnecting..."
                                    } else {
                                        "Confirm disconnect"
                                    }
                                }
                                Button {
                                    class: "btn btn-secondary".to_string(),
                                    onclick: move |_| confirm_disconnect.set(false),
                                    "Cancel"
                                }
                            } else {
                                Button {
                                    class: "btn btn-secondary".to_string(),
                                    onclick: move |_| confirm_disconnect.set(true),
                                    "Disconnect"
                                }
                            }
                        }
                        Link {
                            class: "btn btn-secondary hide-on-mobile",
                            to: crate::Route::BankAccounts {},
                            "Back"
                        }
                    }
                }

                if let Some(err) = action_error() {
                    div { class: "message message-error", "{err}" }
                }
                if let Some(msg) = action_success() {
                    div { class: "message message-success", "{msg}" }
                }

                // Balance comparison.
                match balance_resource() {
                    None => rsx! {
                        div { class: "message message-info", "Loading balance..." }
                    },
                    Some(Err(err)) => rsx! {
                        div { class: "message message-warning", "Could not load bank balance: {err}" }
                    },
                    Some(Ok(cmp)) => rsx! {
                        div { class: "metric-grid",
                            article { class: "metric-card",
                                p { class: "metric-label", "Internal balance" }
                                p { class: "metric-value mono", "{cmp.internal_balance}" }
                                p { class: "tiny-text muted", "From ledger entries" }
                            }
                            article { class: "metric-card",
                                p { class: "metric-label", "Bank balance" }
                                if let Some(bank_bal) = cmp.bank_balance {
                                    p { class: "metric-value mono", "{bank_bal}" }
                                    if let Some(currency) = cmp.bank_balance_currency.as_deref() {
                                        p { class: "tiny-text muted", "{currency} · from last sync" }
                                    }
                                } else {
                                    p { class: "metric-value muted", "Unavailable" }
                                }
                            }
                            article { class: "metric-card",
                                p { class: "metric-label", "Difference" }
                                if let Some(diff) = cmp.difference {
                                    p { class: if diff == Decimal::ZERO { "metric-value text-positive" } else { "metric-value text-negative" },
                                        "{diff}"
                                    }
                                    p { class: "tiny-text muted",
                                        if diff == Decimal::ZERO {
                                            "Balanced"
                                        } else {
                                            "Bank vs. ledger mismatch"
                                        }
                                    }
                                } else {
                                    p { class: "metric-value muted", "-" }
                                }
                            }
                        }
                    },
                }

                // Manual transaction form.
                if is_manual {
                    div { class: "actions-row",
                        Button {
                            class: "btn btn-primary btn-sm".to_string(),
                            onclick: move |_| {
                                let next = !show_add_form();
                                show_add_form.set(next);
                            },
                            if show_add_form() {
                                "Cancel"
                            } else {
                                "Add transaction"
                            }
                        }
                    }
                    if show_add_form() {
                        div {
                            class: "glass-card stack-lg",
                            style: "padding: 1rem;",
                            div { class: "form-grid two-up",
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "new-txn-date",
                                        "Date"
                                    }
                                    input {
                                        id: "new-txn-date",
                                        r#type: "date",
                                        class: "input",
                                        value: new_date().format("%Y-%m-%d").to_string(),
                                        oninput: move |e| {
                                            if let Ok(d) = e.value().parse::<chrono::NaiveDate>() {
                                                new_date.set(d);
                                            }
                                        },
                                    }
                                }
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "new-txn-amount",
                                        "Amount"
                                    }
                                    input {
                                        id: "new-txn-amount",
                                        class: "input",
                                        r#type: "text",
                                        placeholder: "e.g. 150.00 or -50.00",
                                        value: new_amount,
                                        oninput: move |e| new_amount.set(e.value()),
                                    }
                                }
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "new-txn-desc",
                                        "Description"
                                    }
                                    input {
                                        id: "new-txn-desc",
                                        class: "input",
                                        r#type: "text",
                                        placeholder: "e.g. Office supplies",
                                        value: new_description,
                                        oninput: move |e| new_description.set(e.value()),
                                    }
                                }
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "new-txn-ref",
                                        "Reference (optional)"
                                    }
                                    input {
                                        id: "new-txn-ref",
                                        class: "input",
                                        r#type: "text",
                                        placeholder: "e.g. INV-001",
                                        value: new_reference,
                                        oninput: move |e| new_reference.set(e.value()),
                                    }
                                }
                            }
                            if let Some(err) = add_txn_error() {
                                div { class: "message message-error", "{err}" }
                            }
                            div { class: "actions-row",
                                Button {
                                    class: "btn btn-primary".to_string(),
                                    disabled: adding_txn(),
                                    onclick: handle_add_txn,
                                    if adding_txn() {
                                        "Adding..."
                                    } else {
                                        "Save transaction"
                                    }
                                }
                            }
                        }
                    }
                }

                // Filter tabs.
                div { class: "actions-row filter-tabs",
                    Button {
                        class: if filter == "pending" { "btn btn-primary btn-sm".to_string() } else { "btn btn-secondary btn-sm".to_string() },
                        onclick: move |_| status_filter.set("pending"),
                        "Pending ({pending_count})"
                    }
                    Button {
                        class: if filter == "linked" { "btn btn-primary btn-sm".to_string() } else { "btn btn-secondary btn-sm".to_string() },
                        onclick: move |_| status_filter.set("linked"),
                        "Linked ({linked_count})"
                    }
                    Button {
                        class: if filter == "dismissed" { "btn btn-primary btn-sm".to_string() } else { "btn btn-secondary btn-sm".to_string() },
                        onclick: move |_| status_filter.set("dismissed"),
                        "Dismissed ({dismissed_count})"
                    }
                }

                // Transaction list.
                if txns_resource().is_none() {
                    div { class: "message message-info", "Loading transactions..." }
                } else if filtered_txns.is_empty() {
                    div { class: "empty-state",
                        div { class: "empty-icon", "+" }
                        h3 { class: "section-title",
                            if filter == "pending" {
                                "No pending transactions"
                            } else if filter == "linked" {
                                "No linked transactions"
                            } else {
                                "No dismissed transactions"
                            }
                        }
                        p { class: "supporting-text",
                            if filter == "pending" {
                                "All imported transactions have been posted or dismissed."
                            } else if filter == "linked" {
                                "No bank transactions have been linked to journal entries yet."
                            } else {
                                "No transactions have been dismissed."
                            }
                        }
                    }
                } else {
                    table { class: "glass-card data-table stack-rows",
                        thead {
                            tr {
                                th { "Date" }
                                th { "Description" }
                                th { class: "col-right", "Amount" }
                                th { class: "col-right", "Actions" }
                            }
                        }
                        tbody {
                            for txn in filtered_txns {
                                tr {
                                    td { class: "mono muted m-meta", "{txn.date}" }
                                    td { class: "m-title",
                                        div { class: "stack-sm",
                                            span { class: "label-strong", "{txn.description}" }
                                            if let Some(ref r) = txn.reference {
                                                span { class: "tiny-text mono muted",
                                                    "{r}"
                                                }
                                            }
                                        }
                                    }
                                    td { class: "col-right mono m-value",
                                        span { class: if txn.is_inflow() { "text-positive" } else { "text-negative" },
                                            "{txn.amount} {txn.currency}"
                                        }
                                    }
                                    td { class: "col-right m-actions",
                                        div { class: "actions-row justify-end",
                                            if txn.status == "pending" {
                                                Link {
                                                    class: "btn btn-primary btn-sm",
                                                    to: crate::Route::NewTransactionFromBank {
                                                        bank_txn_id: txn.id,
                                                    },
                                                    "Post"
                                                }
                                                if confirm_dismiss() == Some(txn.id) {
                                                    span { class: "tiny-text muted",
                                                        "Dismiss?"
                                                    }
                                                    Button {
                                                        class: "btn btn-danger btn-sm".to_string(),
                                                        disabled: dismissing() == Some(txn.id),
                                                        onclick: move |_| handle_dismiss(txn.id),
                                                        "Yes, dismiss"
                                                    }
                                                    Button {
                                                        class: "btn btn-secondary btn-sm".to_string(),
                                                        onclick: move |_| confirm_dismiss.set(None),
                                                        "Cancel"
                                                    }
                                                } else {
                                                    Button {
                                                        class: "btn btn-secondary btn-sm".to_string(),
                                                        onclick: move |_| confirm_dismiss.set(Some(txn.id)),
                                                        "Dismiss"
                                                    }
                                                }
                                            } else if txn.status == "linked" {
                                                if let Some(txn_id) = txn.linked_transaction_id {
                                                    Link {
                                                        class: "btn btn-secondary btn-sm",
                                                        to: crate::Route::EditTransaction {
                                                            id: txn_id,
                                                        },
                                                        "View journal entry"
                                                    }
                                                }
                                            } else if txn.status == "dismissed" {
                                                Button {
                                                    class: "btn btn-secondary btn-sm".to_string(),
                                                    disabled: restoring() == Some(txn.id),
                                                    onclick: move |_| handle_restore(txn.id),
                                                    if restoring() == Some(txn.id) {
                                                        "Restoring..."
                                                    } else {
                                                        "Restore"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
