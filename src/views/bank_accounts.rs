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

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return Ok(storage.bank_accounts.list(user_id).await?);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[post("/api/bank-accounts/:id/sync")]
pub async fn sync_bank_account(id: Uuid, force: Option<bool>) -> Result<SyncResult, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return crate::server::bank_sync_cache::sync_bank_account_for_user(
            &storage,
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

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        if !storage.bank_accounts.delete(user_id, id).await? {
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
                        p { class: "section-subtitle hide-on-mobile",
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
                    table { class: "glass-card data-table stack-rows",
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
                                    td { class: if account.is_manual { "m-hide" } else { "m-meta m-block" },
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
                                    td { class: "m-title",
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
                                    td {
                                        class: "muted m-meta m-block",
                                        "data-label": "Linked to",
                                        {account.internal_account_name.as_deref().unwrap_or("—")}
                                    }
                                    td {
                                        class: if account.is_manual { "muted m-hide" } else { "muted m-meta m-block" },
                                        "data-label": "Last synced",
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
                                    td { class: "m-value",
                                        if account.is_manual {
                                            span { class: "chip chip-neutral", "Manual" }
                                        } else if account.is_expired() {
                                            span { class: "chip chip-warning", "Expired" }
                                        } else {
                                            span { class: "chip chip-positive", "Active" }
                                        }
                                    }
                                    td { class: "col-right m-actions",
                                        div { class: "actions-row justify-end",
                                            Link {
                                                class: "btn btn-secondary btn-sm",
                                                to: crate::Route::BankAccountDetail {
                                                    id: account.id,
                                                },
                                                "View"
                                            }
                                            Link {
                                                class: "btn btn-secondary btn-sm",
                                                to: crate::Route::BankAccountEdit {
                                                    id: account.id,
                                                },
                                                "Edit"
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
