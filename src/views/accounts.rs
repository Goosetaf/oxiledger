use crate::components::button::Button;
use dioxus::prelude::*;
use uuid::Uuid;

use crate::models::account::Account;

#[get("/api/accounts")]
async fn list_accounts() -> Result<Vec<Account>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return Ok(storage.accounts.list(user_id).await?);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[post("/api/accounts/toggle")]
async fn toggle_account(account_id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        storage.accounts.toggle_active(user_id, account_id).await?;
    }

    Ok(())
}

#[post("/api/accounts/:id/delete")]
async fn delete_account(id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::{
            server::auth::{extract_context, require_auth},
            storage::StorageError,
        };

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let deleted = storage
            .accounts
            .delete(user_id, id)
            .await
            .map_err(|e| match e {
                StorageError::ForeignKeyViolation(_) => ServerFnError::new(
                    "This account is already used in journal entries and cannot be deleted.",
                ),
                other => other.into(),
            })?;

        if !deleted {
            return Err(ServerFnError::new("Account not found"));
        }
    }

    Ok(())
}

#[component]
pub fn Accounts() -> Element {
    let mut accounts_resource = use_loader(list_accounts)?;
    let mut action_error = use_signal(|| None::<String>);

    let handle_toggle = move |id: Uuid| async move {
        action_error.set(None);
        match toggle_account(id).await {
            Ok(_) => accounts_resource.restart(),
            Err(err) => action_error.set(Some(err.to_string())),
        }
    };

    let handle_delete = move |id: Uuid| async move {
        action_error.set(None);
        match delete_account(id).await {
            Ok(_) => accounts_resource.restart(),
            Err(err) => action_error.set(Some(err.to_string())),
        }
    };

    let accounts = accounts_resource.read().clone();

    let total_accounts = accounts.len();
    let active_accounts = accounts.iter().filter(|account| account.is_active).count();
    let inactive_accounts = total_accounts.saturating_sub(active_accounts);

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h1 { class: "section-title", "Accounts" }
                        p { class: "section-subtitle hide-on-mobile",
                            "Manage your ledger accounts and their statuses."
                        }
                    }
                    Link {
                        class: "btn btn-primary",
                        to: crate::Route::NewAccount {},
                        "New account"
                    }
                }

                div { class: "metric-grid hide-on-mobile",
                    article { class: "metric-card",
                        p { class: "metric-label", "Total accounts" }
                        p { class: "metric-value", "{total_accounts}" }
                    }
                    article { class: "metric-card",
                        p { class: "metric-label", "Active accounts" }
                        p { class: "metric-value", "{active_accounts}" }
                    }
                    article { class: "metric-card",
                        p { class: "metric-label", "Inactive accounts" }
                        p { class: "metric-value", "{inactive_accounts}" }
                    }
                }

                if let Some(err) = action_error() {
                    div { class: "message message-error", "{err}" }
                }

                if accounts.is_empty() {
                    div { class: "empty-state",
                        div { class: "empty-icon", "+" }
                        h3 { class: "section-title", "No accounts created yet" }
                        p { class: "supporting-text",
                            "Create your first ledger account to start recording activity."
                        }
                    }
                } else {
                    table { class: "glass-card data-table stack-rows",
                        thead {
                            tr {
                                th { "Code" }
                                th { "Name" }
                                th { "Type" }
                                th { "Normal" }
                                th { "Status" }
                                th { class: "col-right", "Action" }
                            }
                        }
                        tbody {
                            for account in accounts {
                                tr { class: "row-expandable",
                                    td { class: "mono muted m-lead",
                                        {account.code.as_deref().unwrap_or("-")}
                                    }
                                    td { class: "m-title",
                                        div { class: "stack-sm",
                                            label {
                                                class: "label-strong row-toggle-label",
                                                r#for: format!("account-{}-toggle", account.id),
                                                "{account.name}"
                                            }
                                            if let Some(description) = account.description.clone() {
                                                span { class: "tiny-text", "{description}" }
                                            }
                                        }
                                    }
                                    td { class: "muted m-meta", "{account.account_type}" }
                                    td { class: "muted m-hide", "{account.normal_balance}" }
                                    td { class: "m-value",
                                        span { class: if account.is_active { "chip chip-positive" } else { "chip chip-warning" },
                                            if account.is_active {
                                                "Active"
                                            } else {
                                                "Inactive"
                                            }
                                        }
                                        input {
                                            id: format!("account-{}-toggle", account.id),
                                            class: "row-toggle",
                                            r#type: "checkbox",
                                            r#autocomplete: "off",
                                            hidden: true,
                                        }
                                    }
                                    td { class: "col-right m-actions",
                                        div { class: "actions-row justify-end",
                                            Link {
                                                class: "btn btn-secondary btn-sm",
                                                to: crate::Route::EditAccount {
                                                    id: account.id,
                                                },
                                                "Edit"
                                            }
                                            Button {
                                                class: "btn btn-secondary btn-sm".to_string(),
                                                onclick: move |_| handle_toggle(account.id),
                                                if account.is_active {
                                                    "Deactivate"
                                                } else {
                                                    "Activate"
                                                }
                                            }
                                            Button {
                                                class: "btn btn-danger btn-sm".to_string(),
                                                onclick: move |_| handle_delete(account.id),
                                                "Delete"
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
