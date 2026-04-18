use crate::{
    components::button::Button,
    models::bank_sync::{
        BankAccountBalanceComparison, BankAccountSummary, BankTransaction, SyncResult,
    },
};
use dioxus::prelude::*;
use rust_decimal::Decimal;
#[cfg(feature = "server")]
use sqlx::Row;
use uuid::Uuid;

#[get("/api/bank-accounts/:id")]
pub async fn get_bank_account_detail(id: Uuid) -> Result<BankAccountSummary, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let row = sqlx::query!(
            r#"
            SELECT
                ba.id,
                ba.name,
                ba.iban,
                ba.currency,
                ba.internal_account_id,
                ba.is_manual,
                a.name AS internal_account_name,
                bc.aspsp_name,
                bc.aspsp_country,
                bc.provider_id AS "provider_id?: String",
                bc.access_valid_until,
                ba.last_synced_at
            FROM bank_accounts ba
            LEFT JOIN bank_connections bc ON bc.id = ba.bank_connection_id
            JOIN accounts a ON a.id = ba.internal_account_id
            WHERE ba.id = $1 AND ba.user_id = $2
            "#,
            id,
            user_id
        )
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

        return Ok(BankAccountSummary {
            id: row.id,
            name: row.name,
            iban: row.iban,
            currency: row.currency,
            internal_account_id: row.internal_account_id,
            internal_account_name: row.internal_account_name,
            aspsp_name: row.aspsp_name,
            aspsp_country: row.aspsp_country,
            provider_id: row.provider_id,
            access_valid_until: row.access_valid_until,
            last_synced_at: row.last_synced_at,
            is_manual: row.is_manual,
        });
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

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        crate::server::bank_sync_cache::maybe_lazy_sync_for_user(&pool, user_id, id)
            .await
            .map_err(ServerFnError::new)?;

        // Load account and connection.
        let record = sqlx::query(
            r#"
            SELECT
                ba.internal_account_id,
                ba.is_manual,
                ba.balance,
                ba.currency
            FROM bank_accounts ba
            WHERE ba.id = $1 AND ba.user_id = $2
            "#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

        // Compute internal balance: debit_sum - credit_sum of all journal entries
        // for the linked internal account.
        let internal_account_id: Uuid = record.get("internal_account_id");
        let is_manual: bool = record.get("is_manual");

        let bal_row = sqlx::query!(
            r#"
            SELECT
                COALESCE(SUM(CASE WHEN je.entry_type = 'Debit' THEN je.amount ELSE 0::numeric END), 0::numeric)
                    AS "debit_sum!: rust_decimal::Decimal",
                COALESCE(SUM(CASE WHEN je.entry_type = 'Credit' THEN je.amount ELSE 0::numeric END), 0::numeric)
                    AS "credit_sum!: rust_decimal::Decimal"
            FROM journal_entries je
            JOIN transactions t ON t.id = je.transaction_id
            WHERE je.account_id = $1 AND t.user_id = $2
            "#,
            internal_account_id,
            user_id
        )
        .fetch_one(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        let internal_balance = bal_row.debit_sum - bal_row.credit_sum;

        // Manual accounts have no live balance from a provider.
        if is_manual {
            return Ok(BankAccountBalanceComparison {
                bank_account_id: id,
                internal_balance,
                bank_balance: None,
                bank_balance_currency: None,
                difference: None,
            });
        }

        let bank_balance_amount: Option<Decimal> = record.get("balance");
        let bank_balance_currency =
            bank_balance_amount.map(|_| record.get::<String, _>("currency"));

        let difference = bank_balance_amount.map(|b| b - internal_balance);

        return Ok(BankAccountBalanceComparison {
            bank_account_id: id,
            internal_balance,
            bank_balance: bank_balance_amount,
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

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        crate::server::bank_sync_cache::maybe_lazy_sync_for_user(&pool, user_id, id)
            .await
            .map_err(ServerFnError::new)?;

        let rows = sqlx::query_as!(
            BankTransaction,
            r#"
            SELECT
                id, bank_account_id, user_id, external_id,
                date, amount, currency, description, reference,
                status, linked_journal_entry_id, imported_at
            FROM bank_transactions
            WHERE bank_account_id = $1 AND user_id = $2 AND status = 'pending'
            ORDER BY date DESC, imported_at DESC
            "#,
            id,
            user_id
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(rows);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[post("/api/bank-transactions/:id/dismiss")]
pub async fn dismiss_bank_transaction(id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let result = sqlx::query!(
            "UPDATE bank_transactions SET status = 'dismissed'
             WHERE id = $1 AND user_id = $2 AND status = 'pending'",
            id,
            user_id
        )
        .execute(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        if result.rows_affected() == 0 {
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

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let record = sqlx::query!(
            r#"SELECT ba.bank_connection_id, ba.is_manual, bc.provider_id, bc.provider_session_id
            FROM bank_accounts ba
            LEFT JOIN bank_connections bc ON bc.id = ba.bank_connection_id
            WHERE ba.id = $1 AND ba.user_id = $2"#,
            id,
            user_id
        )
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

        if record.is_manual {
            return Err(ServerFnError::new("This bank account is already manual"));
        }

        sqlx::query(
            r#"UPDATE bank_accounts
            SET bank_connection_id = NULL,
                provider_account_uid = NULL,
                is_manual = TRUE,
                last_synced_at = NULL,
                balance = NULL
            WHERE id = $1 AND user_id = $2"#,
        )
        .bind(id)
        .bind(user_id)
        .execute(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        if let Some(connection_id) = record.bank_connection_id {
            let remaining = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM bank_accounts WHERE bank_connection_id = $1",
                connection_id
            )
            .fetch_one(&pool)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?
            .unwrap_or(0);

            if remaining == 0 {
                if let Ok(provider) = crate::bank_sync::get_provider(&record.provider_id).await {
                    let _ = provider.revoke_session(&record.provider_session_id).await;
                }

                sqlx::query!(
                    "DELETE FROM bank_connections WHERE id = $1 AND user_id = $2",
                    connection_id,
                    user_id
                )
                .execute(&pool)
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
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
    let mut txns_resource = use_loader(move || list_bank_transactions(id))?;

    let mut action_error = use_signal(|| None::<String>);
    let mut action_success = use_signal(|| None::<String>);
    let mut syncing = use_signal(|| false);
    let mut dismissing = use_signal(|| None::<Uuid>);
    let mut confirm_dismiss = use_signal(|| None::<Uuid>);
    let mut disconnecting = use_signal(|| false);
    let mut confirm_disconnect = use_signal(|| false);

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

    let txns = txns_resource.read().clone();
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
                            class: "btn btn-secondary",
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

                if txns.is_empty() {
                    div { class: "empty-state",
                        div { class: "empty-icon", "+" }
                        h3 { class: "section-title", "No pending transactions" }
                        p { class: "supporting-text",
                            "All imported transactions have been posted or dismissed."
                        }
                    }
                } else {
                    table { class: "glass-card data-table",
                        thead {
                            tr {
                                th { "Date" }
                                th { "Description" }
                                th { class: "col-right", "Amount" }
                                th { class: "col-right", "Actions" }
                            }
                        }
                        tbody {
                            for txn in txns {
                                tr {
                                    td { class: "mono muted", "{txn.date}" }
                                    td {
                                        div { class: "stack-sm",
                                            span { class: "label-strong", "{txn.description}" }
                                            if let Some(ref r) = txn.reference {
                                                span { class: "tiny-text mono muted",
                                                    "{r}"
                                                }
                                            }
                                        }
                                    }
                                    td { class: "col-right mono",
                                        span { class: if txn.is_inflow() { "text-positive" } else { "text-negative" },
                                            "{txn.amount} {txn.currency}"
                                        }
                                    }
                                    td { class: "col-right",
                                        div { class: "actions-row justify-end",
                                            Link {
                                                class: "btn btn-primary btn-sm",
                                                to: crate::Route::NewTransactionFromBank {
                                                    bank_txn_id: txn.id,
                                                },
                                                "Post"
                                            }
                                            // Dismiss with inline confirmation.
                                            if confirm_dismiss() == Some(txn.id) {
                                                span { class: "tiny-text muted", "Dismiss?" }
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
