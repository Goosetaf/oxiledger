use crate::{
    models::account::{Account, AccountType, CreateAccountRequest, NormalBalance},
    views::account_form::AccountForm,
};
use dioxus::prelude::*;
use uuid::Uuid;

#[cfg(feature = "server")]
use {sqlx::PgPool, tower_cookies::Cookies};

#[get("/api/accounts/:id")]
async fn get_account(id: Uuid) -> Result<Account, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let account = sqlx::query_as!(
            Account,
            r#"SELECT
                id, user_id, name, code,
                account_type AS "account_type: AccountType",
                normal_balance AS "normal_balance: NormalBalance",
                description, is_active
            FROM accounts
            WHERE id = $1 AND user_id = $2"#,
            id,
            user_id
        )
        .fetch_one(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(account);
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[post("/api/accounts/:id")]
async fn update_account(id: Uuid, req: CreateAccountRequest) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        if req.name.trim().is_empty() {
            return Err(ServerFnError::new("Account name cannot be empty"));
        }

        let normal_balance = req.account_type.default_normal_balance();

        let result = sqlx::query!(
            r#"UPDATE accounts
            SET name = $3,
                code = $4,
                account_type = $5,
                normal_balance = $6,
                description = $7,
                updated_at = NOW()
            WHERE id = $1 AND user_id = $2"#,
            id,
            user_id,
            req.name.trim(),
            req.code.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            req.account_type as _,
            normal_balance as _,
            req.description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        )
        .execute(&pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("unique") {
                ServerFnError::new("An account with that code already exists")
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
pub fn EditAccount(id: Uuid) -> Element {
    let nav = use_navigator();
    let account_resource = use_server_future(move || get_account(id))?;

    let account = match account_resource() {
        Some(Ok(account)) => account,
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
                        div { class: "message message-info", "Loading account..." }
                    }
                }
            };
        }
    };

    let name = use_signal(|| account.name.clone());
    let code = use_signal(|| account.code.clone().unwrap_or_default());
    let account_type = use_signal(|| account.account_type);
    let description = use_signal(|| account.description.clone().unwrap_or_default());
    let mut form_error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let nav = nav.clone();
        async move {
            submitting.set(true);
            form_error.set(None);

            let req = CreateAccountRequest {
                name: name(),
                code: {
                    let value = code();
                    if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    }
                },
                account_type: account_type(),
                description: {
                    let value = description();
                    if value.trim().is_empty() {
                        None
                    } else {
                        Some(value)
                    }
                },
            };

            match update_account(id, req).await {
                Ok(_) => {
                    let _ = nav.push(crate::Route::Accounts {});
                }
                Err(err) => form_error.set(Some(err.to_string())),
            }

            submitting.set(false);
        }
    };

    rsx! {
        AccountForm {
            heading: "Edit account".to_string(),
            subtitle: "Update the account details used throughout your chart of accounts.".to_string(),
            submit_label: "Save changes".to_string(),
            submitting_label: "Saving changes...".to_string(),
            name,
            code,
            account_type,
            description,
            form_error,
            submitting,
            onsubmit: handle_submit,
        }
    }
}
