use crate::{
    components::button::Button,
    models::{
        account::Account,
        transaction::{EntryType, JournalEntry},
    },
};
use dioxus::prelude::*;
use rust_decimal::Decimal;
use std::str::FromStr;

#[derive(Clone, PartialEq)]
pub struct EntryRow {
    pub account_id_str: String,
    pub memo: String,
    pub debit_amount_str: String,
    pub credit_amount_str: String,
}

impl EntryRow {
    pub fn new() -> Self {
        Self {
            account_id_str: String::new(),
            memo: String::new(),
            debit_amount_str: String::new(),
            credit_amount_str: String::new(),
        }
    }

    pub fn from_journal_entry(entry: &JournalEntry) -> Self {
        match entry.entry_type {
            EntryType::Debit => Self {
                account_id_str: entry.account_id.to_string(),
                memo: entry.memo.clone().unwrap_or_default(),
                debit_amount_str: entry.amount.to_string(),
                credit_amount_str: String::new(),
            },
            EntryType::Credit => Self {
                account_id_str: entry.account_id.to_string(),
                memo: entry.memo.clone().unwrap_or_default(),
                debit_amount_str: String::new(),
                credit_amount_str: entry.amount.to_string(),
            },
        }
    }

    pub fn parse_amount(amount_str: &str) -> Option<Decimal> {
        Decimal::from_str(amount_str)
            .ok()
            .filter(|amount| *amount > Decimal::ZERO)
    }

    pub fn debit_amount(&self) -> Option<Decimal> {
        Self::parse_amount(&self.debit_amount_str)
    }

    pub fn credit_amount(&self) -> Option<Decimal> {
        Self::parse_amount(&self.credit_amount_str)
    }

    pub fn journal_entry(&self) -> Option<(EntryType, Decimal)> {
        match (self.debit_amount(), self.credit_amount()) {
            (Some(amount), None) => Some((EntryType::Debit, amount)),
            (None, Some(amount)) => Some((EntryType::Credit, amount)),
            _ => None,
        }
    }

    pub fn has_conflicting_amounts(&self) -> bool {
        self.debit_amount().is_some() && self.credit_amount().is_some()
    }
}

fn account_option_label(account: &Account) -> String {
    match account.code.as_deref() {
        Some(code) => format!("{} - {}", code, account.name),
        None => account.name.clone(),
    }
}

#[component]
pub fn TransactionForm(
    heading: String,
    subtitle: String,
    submit_label: String,
    submitting_label: String,
    accounts: Vec<Account>,
    mut txn_date: Signal<String>,
    mut txn_desc: Signal<String>,
    mut txn_ref: Signal<String>,
    mut entry_rows: Signal<Vec<EntryRow>>,
    form_error: ReadSignal<Option<String>>,
    submitting: ReadSignal<bool>,
    onsubmit: Callback<Event<FormData>>,
) -> Element {
    let debit_total = use_memo(move || {
        entry_rows()
            .iter()
            .filter_map(EntryRow::debit_amount)
            .fold(Decimal::ZERO, |sum, amount| sum + amount)
    });

    let credit_total = use_memo(move || {
        entry_rows()
            .iter()
            .filter_map(EntryRow::credit_amount)
            .fold(Decimal::ZERO, |sum, amount| sum + amount)
    });

    let valid_entry_count = use_memo(move || {
        entry_rows()
            .iter()
            .filter(|row| row.journal_entry().is_some() && !row.account_id_str.is_empty())
            .count()
    });

    let has_conflicting_rows =
        use_memo(move || entry_rows().iter().any(EntryRow::has_conflicting_amounts));

    let is_balanced = use_memo(move || {
        !has_conflicting_rows()
            && valid_entry_count() >= 2
            && debit_total() == credit_total()
            && debit_total() > Decimal::ZERO
    });

    let handle_add_row = move |_| {
        entry_rows.write().push(EntryRow::new());
    };

    let mut handle_remove_row = move |index: usize| {
        let mut rows = entry_rows.write();
        if rows.len() > 2 {
            rows.remove(index);
        }
    };

    let submit_button_class = if is_balanced() {
        "btn btn-primary"
    } else {
        "btn btn-secondary"
    };

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
                        h1 { class: "section-title", "{heading}" }
                        p { class: "section-subtitle", "{subtitle}" }
                    }
                }

                section { class: "editor-shell",
                    if accounts.is_empty() {
                        div { class: "message message-info",
                            "You need at least one account before posting a transaction."
                        }
                    } else {
                        form {
                            class: "stack-lg",
                            onsubmit: move |event| onsubmit.call(event),
                            if let Some(err) = form_error() {
                                div { class: "message message-error", "{err}" }
                            }

                            div { class: "form-grid three-up",
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "txn-date",
                                        "Date"
                                    }
                                    input {
                                        id: "txn-date",
                                        class: "input",
                                        r#type: "date",
                                        r#autocomplete: "off",
                                        value: txn_date,
                                        oninput: move |e| txn_date.set(e.value()),
                                        required: true,
                                    }
                                }
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "txn-description",
                                        "Description"
                                    }
                                    input {
                                        id: "txn-description",
                                        class: "input",
                                        r#type: "text",
                                        r#autocomplete: "off",
                                        placeholder: "Monthly office rent",
                                        value: txn_desc,
                                        oninput: move |e| txn_desc.set(e.value()),
                                        required: true,
                                    }
                                }
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "txn-reference",
                                        "Reference"
                                    }
                                    input {
                                        id: "txn-reference",
                                        class: "input",
                                        r#type: "text",
                                        r#autocomplete: "off",
                                        placeholder: "Optional external reference",
                                        value: txn_ref,
                                        oninput: move |e| txn_ref.set(e.value()),
                                    }
                                }
                            }

                            Button {
                                r#type: "button",
                                class: "btn btn-secondary btn-sm".to_string(),
                                onclick: handle_add_row,
                                "+ Add line"
                            }

                            div { class: "entry-list",
                                for (index , row) in entry_rows().iter().cloned().enumerate() {
                                    div { class: "entry-row",
                                        div { class: "entry-grid",
                                            div { class: "field-block",
                                                label { class: "field-label", "Account" }
                                                select {
                                                    class: "select",
                                                    r#autocomplete: "off",
                                                    onchange: move |e| {
                                                        entry_rows.write()[index].account_id_str = e.value();
                                                    },
                                                    option {
                                                        value: "",
                                                        disabled: true,
                                                        selected: row.account_id_str.is_empty(),
                                                        "Select an account"
                                                    }
                                                    for account in accounts.iter() {
                                                        option {
                                                            value: "{account.id}",
                                                            selected: row.account_id_str == account.id.to_string(),
                                                            "{account_option_label(account)}"
                                                        }
                                                    }
                                                }
                                            }

                                            div { class: "field-block",
                                                label { class: "field-label", "Memo" }
                                                input {
                                                    class: "input",
                                                    r#type: "text",
                                                    r#autocomplete: "off",
                                                    placeholder: "Optional note",
                                                    value: row.memo.clone(),
                                                    oninput: move |e| {
                                                        entry_rows.write()[index].memo = e.value();
                                                    },
                                                }
                                            }

                                            div { class: "field-block",
                                                label { class: "field-label", "Debit" }
                                                input {
                                                    class: "input",
                                                    r#type: "text",
                                                    r#autocomplete: "off",
                                                    inputmode: "decimal",
                                                    placeholder: "0.00",
                                                    value: row.debit_amount_str.clone(),
                                                    oninput: move |e| {
                                                        let value = e.value();
                                                        let mut rows = entry_rows.write();
                                                        rows[index].debit_amount_str = value;
                                                        if !rows[index].debit_amount_str.trim().is_empty() {
                                                            rows[index].credit_amount_str.clear();
                                                        }
                                                    },
                                                }
                                            }

                                            div { class: "field-block",
                                                label { class: "field-label", "Credit" }
                                                input {
                                                    class: "input",
                                                    r#type: "text",
                                                    r#autocomplete: "off",
                                                    inputmode: "decimal",
                                                    placeholder: "0.00",
                                                    value: row.credit_amount_str.clone(),
                                                    oninput: move |e| {
                                                        let value = e.value();
                                                        let mut rows = entry_rows.write();
                                                        rows[index].credit_amount_str = value;
                                                        if !rows[index].credit_amount_str.trim().is_empty() {
                                                            rows[index].debit_amount_str.clear();
                                                        }
                                                    },
                                                }
                                            }

                                            div { class: "field-block align-end",
                                                Button {
                                                    r#type: "button",
                                                    class: "btn btn-danger btn-sm".to_string(),
                                                    aria_label: "Remove entry row".to_string(),
                                                    onclick: move |_| handle_remove_row(index),
                                                    "Remove"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "composer-summary",
                                div { class: "summary-metrics",
                                    div { class: "summary-pill",
                                        span { class: "summary-pill-label", "Debits" }
                                        span { class: "summary-pill-value mono", "{debit_total}" }
                                    }
                                    div { class: "summary-pill",
                                        span { class: "summary-pill-label", "Credits" }
                                        span { class: "summary-pill-value mono", "{credit_total}" }
                                    }
                                    div { class: if is_balanced() { "summary-pill chip-positive" } else { "summary-pill chip-negative" },
                                        span { class: "summary-pill-label", "Status" }
                                        span { class: "summary-pill-value",
                                            if has_conflicting_rows() {
                                                "One side per line"
                                            } else if is_balanced() {
                                                "Balanced"
                                            } else {
                                                "Not balanced"
                                            }
                                        }
                                    }
                                }
                                Button {
                                    class: submit_button_class.to_string(),
                                    r#type: "submit",
                                    disabled: !is_balanced() || submitting() || accounts.is_empty(),
                                    "{button_label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
