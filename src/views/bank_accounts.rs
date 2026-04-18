use crate::{
    components::button::Button,
    models::bank_sync::{BankAccountSummary, SyncResult},
};
use dioxus::prelude::*;
use uuid::Uuid;

#[get("/api/bank-accounts")]
pub async fn list_bank_accounts() -> Result<Vec<BankAccountSummary>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let rows = sqlx::query!(
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
            WHERE ba.user_id = $1
            ORDER BY COALESCE(bc.aspsp_name, ba.name), ba.name
            "#,
            user_id
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        let summaries = rows
            .into_iter()
            .map(|r| BankAccountSummary {
                id: r.id,
                name: r.name,
                iban: r.iban,
                currency: r.currency,
                internal_account_id: r.internal_account_id,
                internal_account_name: r.internal_account_name,
                aspsp_name: r.aspsp_name,
                aspsp_country: r.aspsp_country,
                provider_id: r.provider_id,
                access_valid_until: r.access_valid_until,
                last_synced_at: r.last_synced_at,
                is_manual: r.is_manual,
            })
            .collect();

        return Ok(summaries);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[post("/api/bank-accounts/:id/sync")]
pub async fn sync_bank_account(id: Uuid, force: Option<bool>) -> Result<SyncResult, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        return crate::server::bank_sync_cache::sync_bank_account_for_user(
            &pool,
            user_id,
            id,
            force.unwrap_or(false),
        )
        .await
        .map_err(ServerFnError::new);
    }

    #[allow(unreachable_code)]
    Ok(SyncResult {
        new_count: 0,
        skipped_count: 0,
    })
}

#[post("/api/bank-accounts/:id/delete")]
pub async fn delete_bank_account(id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let result = sqlx::query!(
            "DELETE FROM bank_accounts WHERE id = $1 AND user_id = $2",
            id,
            user_id
        )
        .execute(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(ServerFnError::new("Bank account not found"));
        }
    }

    Ok(())
}

// ── UI ─────────────────────────────────────────────────────────────────────────

#[component]
pub fn BankAccounts() -> Element {
    let mut accounts_resource = use_loader(list_bank_accounts)?;
    let mut action_error = use_signal(|| None::<String>);
    let mut action_success = use_signal(|| None::<String>);
    let mut syncing = use_signal(|| None::<Uuid>);

    let handle_sync = move |id: Uuid| async move {
        action_error.set(None);
        action_success.set(None);
        syncing.set(Some(id));

        match sync_bank_account(id, Some(true)).await {
            Ok(result) => {
                action_success.set(Some(format!(
                    "Sync complete: {} new transaction(s), {} already imported.",
                    result.new_count, result.skipped_count
                )));
                accounts_resource.restart();
            }
            Err(err) => action_error.set(Some(err.to_string())),
        }

        syncing.set(None);
    };

    let handle_delete = move |id: Uuid| async move {
        action_error.set(None);
        action_success.set(None);
        match delete_bank_account(id).await {
            Ok(_) => accounts_resource.restart(),
            Err(err) => action_error.set(Some(err.to_string())),
        }
    };

    let accounts = accounts_resource.read().clone();

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h1 { class: "section-title", "Bank Accounts" }
                        p { class: "section-subtitle",
                            "Connected bank accounts and their sync status."
                        }
                    }
                    Link {
                        class: "btn btn-primary",
                        to: crate::Route::BankConnect {
                            source_id: None,
                        },
                        "Connect bank"
                    }
                }

                if let Some(err) = action_error() {
                    div { class: "message message-error", "{err}" }
                }
                if let Some(msg) = action_success() {
                    div { class: "message message-success", "{msg}" }
                }

                if accounts.is_empty() {
                    div { class: "empty-state",
                        div { class: "empty-icon", "+" }
                        h3 { class: "section-title", "No bank accounts connected" }
                        p { class: "supporting-text",
                            "Connect a bank account to start importing transactions."
                        }
                    }
                } else {
                    table { class: "glass-card data-table",
                        thead {
                            tr {
                                th { "Bank" }
                                th { "Account" }
                                th { "Linked to" }
                                th { "Last synced" }
                                th { "Status" }
                                th { class: "col-right", "Actions" }
                            }
                        }
                        tbody {
                            for account in accounts {
                                tr {
                                    td {
                                        div { class: "stack-sm",
                                            if account.is_manual {
                                                span { class: "chip chip-neutral", "Manual" }
                                            } else {
                                                span { class: "label-strong",
                                                    {account.aspsp_name.as_deref().unwrap_or("Unknown bank")}
                                                }
                                                if let Some(country) = account.aspsp_country.as_deref() {
                                                    span { class: "tiny-text muted",
                                                        "{country}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    td {
                                        div { class: "stack-sm",
                                            span { class: "label-strong", {account.display_name()} }
                                            if let Some(iban) = account.iban.as_deref() {
                                                span { class: "tiny-text mono muted",
                                                    "{iban}"
                                                }
                                            }
                                            span { class: "tiny-text muted", "{account.currency}" }
                                        }
                                    }
                                    td { class: "muted", "{account.internal_account_name}" }
                                    td { class: "muted",
                                        if account.is_manual {
                                            "N/A"
                                        } else {
                                            {
                                                account
                                                    .last_synced_at
                                                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                                    .unwrap_or_else(|| "Never".into())
                                            }
                                        }
                                    }
                                    td {
                                        if account.is_manual {
                                            span { class: "chip chip-neutral", "Manual" }
                                        } else if account.is_expired() {
                                            span { class: "chip chip-warning", "Expired" }
                                        } else {
                                            span { class: "chip chip-positive", "Active" }
                                        }
                                    }
                                    td { class: "col-right",
                                        div { class: "actions-row justify-end",
                                            Link {
                                                class: "btn btn-secondary btn-sm",
                                                to: crate::Route::BankAccountDetail {
                                                    id: account.id,
                                                },
                                                "View"
                                            }
                                            if account.is_manual {
                                                Link {
                                                    class: "btn btn-secondary btn-sm",
                                                    to: crate::Route::BankConnect {
                                                        source_id: Some(account.id),
                                                    },
                                                    "Connect"
                                                }
                                            } else {
                                                Button {
                                                    class: "btn btn-secondary btn-sm".to_string(),
                                                    disabled: syncing() == Some(account.id),
                                                    onclick: move |_| handle_sync(account.id),
                                                    if syncing() == Some(account.id) {
                                                        "Syncing..."
                                                    } else {
                                                        "Sync"
                                                    }
                                                }
                                                if account.is_expired() {
                                                    Link {
                                                        class: "btn btn-secondary btn-sm",
                                                        to: crate::Route::BankConnect {
                                                            source_id: None,
                                                        },
                                                        "Reconnect"
                                                    }
                                                }
                                            }
                                            Button {
                                                class: "btn btn-danger btn-sm".to_string(),
                                                onclick: move |_| handle_delete(account.id),
                                                "Remove"
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
