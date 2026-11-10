use crate::{
    models::account::{Account, CreateAccountRequest},
    views::account_form::AccountForm,
};
use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::storage::{ledger::AccountDraft, StorageError};

/// Checks and tidies what the account form sent.
#[cfg(feature = "server")]
pub(crate) fn account_draft(req: CreateAccountRequest) -> Result<AccountDraft, ServerFnError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ServerFnError::new("Account name cannot be empty"));
    }

    let non_empty = |value: Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    Ok(AccountDraft {
        name: name.to_string(),
        code: non_empty(req.code),
        account_type: req.account_type,
        normal_balance: req.account_type.default_normal_balance(),
        description: non_empty(req.description),
    })
}

#[cfg(feature = "server")]
pub(crate) fn account_save_error(err: StorageError) -> ServerFnError {
    match err {
        StorageError::UniqueViolation(_) => {
            ServerFnError::new("An account with that code already exists")
        }
        other => other.into(),
    }
}

#[post("/api/accounts")]
pub async fn create_account(req: CreateAccountRequest) -> Result<Account, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let draft = account_draft(req)?;

        return storage
            .ledger
            .create_account(user_id, draft)
            .await
            .map_err(account_save_error);
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
