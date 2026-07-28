use crate::{models::account::Account, models::bank_sync::BankAccountRecord};
use dioxus::prelude::*;
use uuid::Uuid;

#[get("/api/bank-accounts/:id/edit")]
async fn get_bank_account_for_edit(id: Uuid) -> Result<BankAccountRecord, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let account = sqlx::query_as::<_, BankAccountRecord>(
            r#"SELECT id, user_id, bank_connection_id, internal_account_id,
                   provider_account_uid, iban, name, currency, balance, last_synced_at, created_at
               FROM bank_accounts
               WHERE id = $1 AND user_id = $2"#,
        )
        .bind(id)
        .bind(user_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Bank account not found"))?;

        return Ok(account);
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[post("/api/bank-accounts/:id/edit")]
async fn update_bank_account(
    id: Uuid,
    name: String,
    iban: Option<String>,
    currency: String,
    internal_account_id: Option<Uuid>,
) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let trimmed_name = name.trim();
        if trimmed_name.is_empty() {
            return Err(ServerFnError::new("Bank account name cannot be empty"));
        }

        let trimmed_currency = currency.trim().to_uppercase();
        if trimmed_currency.is_empty() {
            return Err(ServerFnError::new("Currency cannot be empty"));
        }

        let result = sqlx::query(
            r#"UPDATE bank_accounts
               SET name = $3,
                   iban = $4,
                   currency = $5,
                   internal_account_id = $6
               WHERE id = $1 AND user_id = $2"#,
        )
        .bind(id)
        .bind(user_id)
        .bind(trimmed_name)
        .bind(
            iban.as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )
        .bind(trimmed_currency)
        .bind(internal_account_id)
        .execute(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(ServerFnError::new("Bank account not found"));
        }
    }

    Ok(())
}

#[component]
pub fn BankAccountEdit(id: Uuid) -> Element {
    let nav = use_navigator();
    let account_resource = use_server_future(move || get_bank_account_for_edit(id))?;
    let internal_accounts_resource = use_loader(crate::views::transactions::list_accounts_for_txn)?;

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
                        div { class: "message message-info", "Loading bank account..." }
                    }
                }
            };
        }
    };

    let internal_accounts: Vec<Account> = internal_accounts_resource.read().clone();
    let mut name = use_signal(|| account.name.clone().unwrap_or_default());
    let mut iban = use_signal(|| account.iban.clone().unwrap_or_default());
    let mut currency = use_signal(|| account.currency.clone());
    let mut internal_account_id = use_signal(|| {
        account.internal_account_id.map(|id| id.to_string()).unwrap_or_default()
    });
    let mut form_error = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);

    let handle_submit = move |event: Event<FormData>| {
        event.prevent_default();
        let nav = nav.clone();
        async move {
            submitting.set(true);
            form_error.set(None);

            let selected_internal_account = if internal_account_id().trim().is_empty() {
                None
            } else {
                match Uuid::parse_str(&internal_account_id()) {
                    Ok(value) => Some(value),
                    Err(_) => {
                        form_error.set(Some("Invalid ledger account selection".to_string()));
                        submitting.set(false);
                        return;
                    }
                }
            };

            let iban_value = {
                let value = iban();
                if value.trim().is_empty() {
                    None
                } else {
                    Some(value)
                }
            };

            match update_bank_account(
                id,
                name(),
                iban_value,
                currency(),
                selected_internal_account,
            )
            .await
            {
                Ok(_) => {
                    let _ = nav.push(crate::Route::BankAccountDetail { id });
                }
                Err(err) => form_error.set(Some(err.to_string())),
            }

            submitting.set(false);
        }
    };

    let button_label = if submitting() {
        "Saving changes..."
    } else {
        "Save changes"
    };

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h2 { class: "section-title", "Edit bank account" }
                        p { class: "section-subtitle",
                            "Update the imported bank account details and linked ledger account."
                        }
                    }
                }

                section { class: "section-card",
                    if let Some(err) = form_error() {
                        div { class: "message message-error", "{err}" }
                    }

                    form { class: "stack-lg", onsubmit: handle_submit,
                        div { class: "form-grid two-up",
                            div { class: "field-block",
                                label {
                                    class: "field-label",
                                    r#for: "bank-account-name",
                                    "Name"
                                }
                                input {
                                    id: "bank-account-name",
                                    class: "input",
                                    r#type: "text",
                                    value: name,
                                    oninput: move |e| name.set(e.value()),
                                    required: true,
                                }
                            }

                            div { class: "field-block",
                                label {
                                    class: "field-label",
                                    r#for: "bank-account-currency",
                                    "Currency"
                                }
                                input {
                                    id: "bank-account-currency",
                                    class: "input",
                                    r#type: "text",
                                    maxlength: 3,
                                    value: currency,
                                    oninput: move |e| currency.set(e.value().to_uppercase()),
                                    required: true,
                                }
                            }
                        }

                        div { class: "form-grid two-up",
                            div { class: "field-block",
                                label {
                                    class: "field-label",
                                    r#for: "bank-account-iban",
                                    "IBAN"
                                }
                                input {
                                    id: "bank-account-iban",
                                    class: "input",
                                    r#type: "text",
                                    value: iban,
                                    oninput: move |e| iban.set(e.value()),
                                }
                            }

                            div { class: "field-block",
                                label {
                                    class: "field-label",
                                    r#for: "bank-account-linked-account",
                                    "Linked ledger account"
                                }
                                select {
                                    id: "bank-account-linked-account",
                                    class: "select",
                                    value: internal_account_id,
                                    onchange: move |e| internal_account_id.set(e.value()),
                                    option {
                                        value: "",
                                        selected: internal_account_id().is_empty(),
                                        "No linked account"
                                    }
                                    for account in internal_accounts.iter() {
                                        option {
                                            value: "{account.id}",
                                            selected: internal_account_id() == account.id.to_string(),
                                            {
                                                match account.code.as_deref() {
                                                    Some(code) => format!("{} - {}", code, account.name),
                                                    None => account.name.clone(),
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div { class: "actions-row justify-end",
                            Link {
                                class: "btn btn-secondary",
                                to: crate::Route::BankAccountDetail {
                                    id,
                                },
                                "Cancel"
                            }
                            button {
                                class: "btn btn-primary",
                                r#type: "submit",
                                disabled: submitting(),
                                "{button_label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
