use crate::{
    models::account::{Account, CreateAccountRequest},
    views::account_form::AccountForm,
};
use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::models::account::{AccountType, NormalBalance};

#[post("/api/accounts")]
pub async fn create_account(req: CreateAccountRequest) -> Result<Account, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        if req.name.trim().is_empty() {
            return Err(ServerFnError::new("Account name cannot be empty"));
        }

        let normal_balance = req.account_type.default_normal_balance();

        let account = sqlx::query_as!(
            Account,
            r#"INSERT INTO accounts (user_id, name, code, account_type, normal_balance, description)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING
                id, user_id, name, code,
                account_type AS "account_type: AccountType",
                normal_balance AS "normal_balance: NormalBalance",
                description, is_active"#,
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
        .fetch_one(&pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("unique") {
                ServerFnError::new("An account with that code already exists")
            } else {
                ServerFnError::new(e.to_string())
            }
        })?;

        return Ok(account);
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[component]
pub fn NewAccount() -> Element {
    let nav = use_navigator();
    let new_name = use_signal(String::new);
    let new_code = use_signal(String::new);
    let new_type = use_signal(|| crate::models::account::AccountType::Asset);
    let new_desc = use_signal(String::new);
    let mut form_error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);

    let handle_create = move |e: Event<FormData>| {
        e.prevent_default();
        let nav = nav.clone();
        async move {
            submitting.set(true);
            form_error.set(None);

            let req = CreateAccountRequest {
                name: new_name(),
                code: {
                    let code = new_code();
                    if code.trim().is_empty() {
                        None
                    } else {
                        Some(code)
                    }
                },
                account_type: new_type(),
                description: {
                    let desc = new_desc();
                    if desc.trim().is_empty() {
                        None
                    } else {
                        Some(desc)
                    }
                },
            };

            match create_account(req).await {
                Ok(_) => {
                    let _ = nav.push(crate::Route::Accounts {});
                }
                Err(e) => form_error.set(Some(e.to_string())),
            }

            submitting.set(false);
        }
    };

    rsx! {
        AccountForm {
            heading: "Add an account".to_string(),
            subtitle: "Use clear codes and descriptions to keep reporting tidy later.".to_string(),
            submit_label: "Add account".to_string(),
            submitting_label: "Adding account...".to_string(),
            name: new_name,
            code: new_code,
            account_type: new_type,
            description: new_desc,
            form_error,
            submitting,
            onsubmit: handle_create,
        }
    }
}
