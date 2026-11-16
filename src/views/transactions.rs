use crate::components::button::Button;
use dioxus::prelude::*;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::models::{
    account::Account,
    transaction::{EntryType, Transaction},
};

#[get("/api/transactions")]
async fn list_transactions() -> Result<Vec<Transaction>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return Ok(storage.transactions.list(user_id).await?);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[get("/api/transactions/accounts")]
pub async fn list_accounts_for_txn() -> Result<Vec<Account>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        return Ok(storage.accounts.list_active(user_id).await?);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[post("/api/transactions/:id/delete")]
async fn delete_transaction(id: Uuid) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (storage, cookies) = extract_context().await?;
        let user_id = require_auth(&storage, &cookies).await?;

        if !storage.transactions.delete(user_id, id).await? {
            return Err(ServerFnError::new("Transaction not found"));
        }
    }

    Ok(())
}

fn entry_account_label(entry: &crate::models::transaction::JournalEntry) -> String {
    match (entry.account_code.as_deref(), entry.account_name.as_deref()) {
        (Some(code), Some(name)) => format!("{} - {}", code, name),
        (_, Some(name)) => name.to_string(),
        _ => "Unknown account".to_string(),
    }
}

fn transaction_total(txn: &Transaction) -> Decimal {
    txn.entries.iter().fold(Decimal::ZERO, |sum, entry| {
        if entry.entry_type == EntryType::Debit {
            sum + entry.amount
        } else {
            sum
        }
    })
}

#[component]
pub fn Transactions() -> Element {
    let mut txns_resource = use_loader(list_transactions)?;
    let accounts = use_loader(list_accounts_for_txn)?.read().clone();
    let mut action_error = use_signal(|| None::<String>);

    let handle_delete = move |id: Uuid| async move {
        action_error.set(None);
        match delete_transaction(id).await {
            Ok(_) => txns_resource.restart(),
            Err(err) => action_error.set(Some(err.to_string())),
        }
    };

    let txns = txns_resource.read().clone();

    let transaction_count = txns.len();
    let line_count: usize = txns.iter().map(|txn| txn.entries.len()).sum();
    let transaction_rows: Vec<_> = txns
        .iter()
        .map(|txn| (txn.id, txn.clone(), transaction_total(txn)))
        .collect();

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h1 { class: "section-title", "Transactions" }
                        p { class: "section-subtitle hide-on-mobile",
                            "Review activity and create manual entries."
                        }
                    }
                    // Phones use bottom bar instead
                    Link {
                        class: "btn btn-primary hide-on-mobile",
                        to: crate::Route::NewTransaction {},
                        "New transaction"
                    }
                }

                div { class: "metric-grid hide-on-mobile",
                    article { class: "metric-card",
                        p { class: "metric-label", "Transactions" }
                        p { class: "metric-value", "{transaction_count}" }
                    }
                    article { class: "metric-card",
                        p { class: "metric-label", "Entry Lines" }
                        p { class: "metric-value", "{line_count}" }
                    }
                    article { class: "metric-card",
                        p { class: "metric-label", "Active Accounts" }
                        p { class: "metric-value", "{accounts.len()}" }
                    }
                }

                section { class: "glass-card",

                    if let Some(err) = action_error() {
                        div { class: "message message-error", "{err}" }
                    }

                    if txns.is_empty() {
                        div { class: "empty-state",
                            div { class: "empty-icon", "+" }
                            h3 { class: "section-title", "No transactions yet" }
                            p { class: "supporting-text",
                                "Open the composer and post your first balanced transaction."
                            }
                        }
                    } else {
                        table { class: "data-table transaction-table stack-rows",
                            thead {
                                tr {
                                    th { "Date" }
                                    th { "Transaction" }
                                    th { "Reference" }
                                    th { class: "!text-right", "Amount" }
                                }
                            }
                            tbody {
                                for (txn_id , txn , total) in transaction_rows {
                                    tr { class: "transaction-summary-row row-expandable",
                                        td { class: "transaction-summary-cell mono muted m-meta",
                                            label {
                                                class: "transaction-row-button",
                                                r#for: format!("txn-{}-toggle", txn_id),
                                                "{txn.date}"
                                            }
                                        }
                                        td { class: "transaction-summary-cell label-strong m-title",
                                            label {
                                                class: "transaction-row-button",
                                                r#for: format!("txn-{}-toggle", txn_id),
                                                "{txn.description}"
                                            }
                                        }
                                        td { class: "transaction-summary-cell muted m-meta",
                                            label {
                                                class: "transaction-row-button",
                                                r#for: format!("txn-{}-toggle", txn_id),
                                                {txn.reference.clone().unwrap_or_else(|| "".to_string())}
                                            }
                                        }
                                        td { class: "transaction-summary-cell col-right mono label-strong m-value",
                                            label {
                                                class: "transaction-row-button",
                                                r#for: format!("txn-{}-toggle", txn_id),
                                                "{total}"
                                            }
                                            input {
                                                id: format!("txn-{}-toggle", txn.id),
                                                class: "transaction-toggle row-toggle",
                                                r#type: "checkbox",
                                                r#autocomplete: "off",
                                                hidden: true,
                                            }
                                        }
                                    }

                                    tr { class: "transaction-detail-row",
                                        td { colspan: 4,
                                            div { class: "transaction-detail-panel",
                                                table { class: "transaction-entry-table",
                                                    thead {
                                                        tr {
                                                            th { "Account" }
                                                            th { class: "!text-right",
                                                                "Debit"
                                                            }
                                                            th { class: "!text-right",
                                                                "Credit"
                                                            }
                                                        }
                                                    }
                                                    tbody {
                                                        for entry in txn.entries {
                                                            tr {
                                                                td {
                                                                    div { class: "stack-sm",
                                                                        span { class: "entry-line-title",
                                                                            "{entry_account_label(&entry)}"
                                                                        }
                                                                        if let Some(memo) = entry.memo.clone() {
                                                                            span { class: "entry-line-meta",
                                                                                "{memo}"
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                                td { class: "mono label-strong text-right",
                                                                    if entry.entry_type == EntryType::Debit {
                                                                        "{entry.amount}"
                                                                    } else {
                                                                        ""
                                                                    }
                                                                }
                                                                td { class: "mono label-strong text-right",
                                                                    if entry.entry_type == EntryType::Credit {
                                                                        "{entry.amount}"
                                                                    } else {
                                                                        ""
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                div { class: "actions-row justify-end",
                                                    Link {
                                                        class: "btn btn-secondary btn-sm",
                                                        to: crate::Route::EditTransaction {
                                                            id: txn.id,
                                                        },
                                                        "Edit"
                                                    }
                                                    Button {
                                                        class: "btn btn-danger btn-sm".to_string(),
                                                        onclick: move |_| handle_delete(txn.id),
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
    }
}
