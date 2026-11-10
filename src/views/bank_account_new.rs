use crate::models::account::Account;
use dioxus::prelude::*;
use uuid::Uuid;

/// Server function: create a manual bank account (no external provider).
#[post("/api/bank-accounts/manual")]
pub async fn create_manual_bank_account(
    name: String,
    iban: Option<String>,
    currency: String,
    internal_account_id: Option<Uuid>,
) -> Result<Uuid, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::{
            server::auth::{extract_context, require_auth},
            storage::bank::BankAccountDetails,
        };

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        let name_trimmed = name.trim().to_string();
        if name_trimmed.is_empty() {
            return Err(ServerFnError::new("Account name is required"));
        }

        let currency_trimmed = currency.trim().to_uppercase();
        if currency_trimmed.is_empty() {
            return Err(ServerFnError::new("Currency is required"));
        }

        // Verify the selected ledger account belongs to this user (only when provided).
        if let Some(account_id) = internal_account_id {
            if storage
                .ledger
                .find_account(user_id, account_id)
                .await?
                .is_none()
            {
                return Err(ServerFnError::new("Ledger account not found"));
            }
        }

        let details = BankAccountDetails {
            name: name_trimmed,
            iban: iban.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
            currency: currency_trimmed,
            internal_account_id,
        };

        return Ok(storage.bank.create_manual_account(user_id, details).await?);
    }

    #[allow(unreachable_code)]
    Err(ServerFnError::new("Server only"))
}

#[derive(Clone, PartialEq)]
pub struct ManualBankAccountFields {
    pub name: String,
    pub iban: Option<String>,
    pub currency: String,
    pub internal_account_id: Option<Uuid>,
}

#[component]
pub fn ManualBankAccountForm(
    internal_accounts: Vec<Account>,
    submitting: bool,
    onsubmit: EventHandler<ManualBankAccountFields>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut iban = use_signal(String::new);
    let mut currency = use_signal(|| "EUR".to_string());
    let mut internal_account_id_str = use_signal(String::new);
    let mut local_error = use_signal(|| None::<String>);

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        local_error.set(None);

        let internal_account_id = if internal_account_id_str().is_empty() {
            None
        } else {
            match Uuid::parse_str(&internal_account_id_str()) {
                Ok(id) => Some(id),
                Err(_) => {
                    local_error.set(Some("Invalid ledger account selection.".to_string()));
                    return;
                }
            }
        };

        onsubmit.call(ManualBankAccountFields {
            name: name(),
            iban: {
                let value = iban();
                if value.trim().is_empty() {
                    None
                } else {
                    Some(value)
                }
            },
            currency: currency(),
            internal_account_id,
        });
    };

    rsx! {
        if let Some(err) = local_error() {
            div { class: "message message-error", "{err}" }
        }

        form { class: "stack-lg", onsubmit: handle_submit,

            div { class: "form-grid two-up",
                div { class: "field-block",
                    label { class: "field-label", r#for: "ba-name", "Account name" }
                    input {
                        id: "ba-name",
                        class: "input",
                        r#type: "text",
                        placeholder: "e.g. Main Checking",
                        required: true,
                        value: name,
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div { class: "field-block",
                    label { class: "field-label", r#for: "ba-currency", "Currency" }
                    input {
                        id: "ba-currency",
                        class: "input",
                        r#type: "text",
                        placeholder: "EUR",
                        maxlength: 10,
                        required: true,
                        value: currency,
                        oninput: move |e| currency.set(e.value()),
                    }
                }
                div { class: "field-block",
                    label { class: "field-label", r#for: "ba-iban", "IBAN (optional)" }
                    input {
                        id: "ba-iban",
                        class: "input",
                        r#type: "text",
                        placeholder: "e.g. FI21 1234 5600 0007 85",
                        value: iban,
                        oninput: move |e| iban.set(e.value()),
                    }
                }
                div { class: "field-block",
                    label { class: "field-label", r#for: "ba-account", "Ledger account (optional)" }
                    select {
                        id: "ba-account",
                        class: "select",
                        onchange: move |e| internal_account_id_str.set(e.value()),
                        option {
                            value: "",
                            selected: internal_account_id_str().is_empty(),
                            "No linked account"
                        }
                        for acc in internal_accounts.iter() {
                            option {
                                value: "{acc.id}",
                                selected: internal_account_id_str() == acc.id.to_string(),
                                {
                                    match acc.code.as_deref() {
                                        Some(code) => format!("{} - {}", code, acc.name),
                                        None => acc.name.clone(),
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "actions-row justify-end",
                button {
                    class: "btn btn-primary",
                    r#type: "submit",
                    disabled: submitting,
                    if submitting {
                        "Saving..."
                    } else {
                        "Save manual account"
                    }
                }
            }
        }
    }
}

#[component]
pub fn NewBankAccount() -> Element {
    rsx! {
        crate::views::bank_connect::BankConnect { source_id: None }
    }
}
