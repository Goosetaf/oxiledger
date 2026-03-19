use crate::components::button::Button;
use dioxus::prelude::*;
use uuid::Uuid;

#[cfg(feature = "server")]
use {
    crate::models::account::{AccountType, NormalBalance},
    sqlx::PgPool,
    tower_cookies::Cookies,
};

use crate::models::account::Account;

#[get("/api/accounts")]
async fn list_accounts() -> Result<Vec<Account>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let accounts = sqlx::query_as!(
            Account,
            r#"SELECT
                id, user_id, name, code,
                account_type AS "account_type: AccountType",
                normal_balance AS "normal_balance: NormalBalance",
                description, is_active
            FROM accounts
            WHERE user_id = $1
            ORDER BY account_type, COALESCE(code, ''), name"#,
            user_id
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(accounts);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[post("/api/accounts/toggle")]
async fn toggle_account(account_id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        sqlx::query!(
            "UPDATE accounts SET is_active = NOT is_active, updated_at = NOW()
             WHERE id = $1 AND user_id = $2",
            account_id,
            user_id
        )
        .execute(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    }

    Ok(())
}

#[post("/api/accounts/:id/delete")]
async fn delete_account(id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let result = sqlx::query!(
            "DELETE FROM accounts WHERE id = $1 AND user_id = $2",
            id,
            user_id
        )
        .execute(&pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("journal_entries") || e.to_string().contains("foreign key") {
                ServerFnError::new(
                    "This account is already used in journal entries and cannot be deleted.",
                )
            } else {
                ServerFnError::new(e.to_string())
            }
        })?;

        if result.rows_affected() == 0 {
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
                        p { class: "section-subtitle",
                            "Manage your ledger accounts and their statuses."
                        }
                    }
                    Link {
                        class: "btn btn-primary",
                        to: crate::Route::NewAccount {},
                        "New account"
                    }
                }

                div { class: "metric-grid",
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
                    table { class: "glass-card data-table",
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
                                tr {
                                    td { class: "mono muted",
                                        {account.code.as_deref().unwrap_or("-")}
                                    }
                                    td {
                                        div { class: "stack-sm",
                                            span { class: "label-strong", "{account.name}" }
                                            if let Some(description) = account.description.clone() {
                                                span { class: "tiny-text", "{description}" }
                                            }
                                        }
                                    }
                                    td { class: "muted", "{account.account_type}" }
                                    td { class: "muted", "{account.normal_balance}" }
                                    td {
                                        span { class: if account.is_active { "chip chip-positive" } else { "chip chip-warning" },
                                            if account.is_active {
                                                "Active"
                                            } else {
                                                "Inactive"
                                            }
                                        }
                                    }
                                    td { class: "col-right",
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
