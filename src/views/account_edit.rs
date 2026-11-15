use crate::{
    models::account::{Account, CreateAccountRequest},
    views::account_form::AccountForm,
};
use dioxus::prelude::*;
use uuid::Uuid;

#[get("/api/accounts/:id")]
async fn get_account(id: Uuid) -> Result<Account, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return storage
            .accounts
            .find(user_id, id)
            .await?
            .ok_or_else(|| ServerFnError::new("Account not found"));
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[post("/api/accounts/:id")]
async fn update_account(id: Uuid, req: CreateAccountRequest) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::{
            server::auth::{extract_context, require_auth},
            views::account_new::{account_draft, account_save_error},
        };

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let draft = account_draft(req)?;

        let updated = storage
            .accounts
            .update(user_id, id, draft)
            .await
            .map_err(account_save_error)?;

        if !updated {
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
