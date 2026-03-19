use crate::{components::button::Button, models::account::AccountType};
use dioxus::prelude::*;

#[component]
pub fn AccountForm(
    heading: String,
    subtitle: String,
    submit_label: String,
    submitting_label: String,
    mut name: Signal<String>,
    mut code: Signal<String>,
    mut account_type: Signal<AccountType>,
    mut description: Signal<String>,
    form_error: ReadSignal<Option<String>>,
    submitting: ReadSignal<bool>,
    onsubmit: Callback<Event<FormData>>,
) -> Element {
    let button_label = if submitting() {
        submitting_label
    } else {
        submit_label
    };

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h2 { class: "section-title", "{heading}" }
                        p { class: "section-subtitle", "{subtitle}" }
                    }
                }

                section { class: "section-card",
                    if let Some(err) = form_error() {
                        div { class: "message message-error", "{err}" }
                    }

                    form {
                        class: "stack-lg",
                        onsubmit: move |event| onsubmit.call(event),
                        div { class: "form-grid two-up",
                            div { class: "field-block",
                                label {
                                    class: "field-label",
                                    r#for: "account-code",
                                    "Code"
                                }
                                input {
                                    id: "account-code",
                                    class: "input",
                                    r#type: "text",
                                    r#autocomplete: "off",
                                    placeholder: "1000",
                                    value: code,
                                    oninput: move |e| code.set(e.value()),
                                }
                                span { class: "field-note", "Optional, but useful for clean reporting." }
                            }

                            div { class: "field-block",
                                label {
                                    class: "field-label",
                                    r#for: "account-name",
                                    "Name"
                                }
                                input {
                                    id: "account-name",
                                    class: "input",
                                    r#type: "text",
                                    r#autocomplete: "off",
                                    placeholder: "Cash",
                                    value: name,
                                    oninput: move |e| name.set(e.value()),
                                    required: true,
                                }
                            }
                        }

                        div { class: "form-grid two-up",
                            div { class: "field-block",
                                label {
                                    class: "field-label",
                                    r#for: "account-type",
                                    "Type"
                                }
                                select {
                                    id: "account-type",
                                    class: "select",
                                    r#autocomplete: "off",
                                    onchange: move |e| {
                                        account_type
                                            .set(
                                                match e.value().as_str() {
                                                    "Asset" => AccountType::Asset,
                                                    "Liability" => AccountType::Liability,
                                                    "Equity" => AccountType::Equity,
                                                    "Revenue" => AccountType::Revenue,
                                                    "Expense" => AccountType::Expense,
                                                    _ => AccountType::Asset,
                                                },
                                            );
                                    },
                                    option {
                                        value: "Asset",
                                        selected: matches!(account_type(), AccountType::Asset),
                                        "Asset"
                                    }
                                    option {
                                        value: "Liability",
                                        selected: matches!(account_type(), AccountType::Liability),
                                        "Liability"
                                    }
                                    option {
                                        value: "Equity",
                                        selected: matches!(account_type(), AccountType::Equity),
                                        "Equity"
                                    }
                                    option {
                                        value: "Revenue",
                                        selected: matches!(account_type(), AccountType::Revenue),
                                        "Revenue"
                                    }
                                    option {
                                        value: "Expense",
                                        selected: matches!(account_type(), AccountType::Expense),
                                        "Expense"
                                    }
                                }
                            }

                            div { class: "field-block",
                                label { class: "field-label", "Default normal balance" }
                                div {
                                    class: "input",
                                    style: "display:flex; align-items:center;",
                                    "{account_type().default_normal_balance()}"
                                }
                            }
                        }

                        div { class: "field-block",
                            label {
                                class: "field-label",
                                r#for: "account-description",
                                "Description"
                            }
                            textarea {
                                id: "account-description",
                                class: "textarea",
                                r#autocomplete: "off",
                                placeholder: "Add a quick note about the account's purpose",
                                value: description,
                                oninput: move |e| description.set(e.value()),
                            }
                        }

                        div { class: "actions-row justify-end",
                            Button {
                                class: "btn btn-primary".to_string(),
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
