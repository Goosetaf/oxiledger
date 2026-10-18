use crate::{
    milestones::{get_progress, record_milestone, Hint, Milestone, Progress},
    models::transaction::AccountBalance,
};
use dioxus::prelude::*;
use rust_decimal::Decimal;

#[get("/api/dashboard/balances")]
async fn get_account_balances() -> Result<Vec<AccountBalance>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let rows = sqlx::query!(
            r#"
            SELECT
                a.id,
                a.name,
                a.code,
                a.normal_balance::text AS "normal_balance!: String",
                COALESCE(SUM(CASE WHEN je.entry_type = 'Debit' THEN je.amount ELSE 0::numeric END), 0::numeric)
                    AS "debit_sum!: rust_decimal::Decimal",
                COALESCE(SUM(CASE WHEN je.entry_type = 'Credit' THEN je.amount ELSE 0::numeric END), 0::numeric)
                    AS "credit_sum!: rust_decimal::Decimal"
            FROM accounts a
            LEFT JOIN journal_entries je ON je.account_id = a.id
            WHERE a.user_id = $1 AND a.is_active = TRUE
            GROUP BY a.id, a.name, a.code, a.normal_balance
            ORDER BY a.account_type, COALESCE(a.code, ''), a.name
            "#,
            user_id
        )
        .fetch_all(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        let balances = rows
            .into_iter()
            .map(|r| {
                let balance = r.debit_sum - r.credit_sum;

                AccountBalance {
                    account_id: r.id,
                    account_name: r.name,
                    account_code: r.code,
                    balance,
                }
            })
            .collect();

        return Ok(balances);
    }

    #[allow(unreachable_code)]
    Ok(vec![])
}

#[component]
pub fn Dashboard() -> Element {
    let balances = use_loader(get_account_balances)?.read().clone();
    // Loaded after the balances, as two server futures at once stall the server render.
    // If it fails the dashboard still works, just without the checklist.
    let progress = use_server_future(get_progress)?().and_then(Result::ok);

    if progress.as_ref().is_some_and(Progress::needs_setup) {
        return rsx! {
            OpenSetup {}
        };
    }

    let active_accounts = balances.len();
    let positive_accounts = balances
        .iter()
        .filter(|balance| balance.balance >= Decimal::ZERO)
        .count();
    let total_balance = balances
        .iter()
        .fold(Decimal::ZERO, |acc, balance| acc + balance.balance);

    let balance_tone = if total_balance >= Decimal::ZERO {
        "metric-value text-positive"
    } else {
        "metric-value text-negative"
    };

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h1 { class: "section-title", "Dashboard" }
                        p { class: "section-subtitle hide-on-mobile",
                            "Overview of your account balances and metrics."
                        }
                    }
                }

                if let Some(progress) = progress {
                    GettingStarted { progress }
                }

                div { class: "metric-grid",
                    article { class: "metric-card",
                        p { class: "metric-label", "Active Accounts" }
                        p { class: "metric-value", "{active_accounts}" }
                    }
                    article { class: "metric-card",
                        p { class: "metric-label", "Positive Balances" }
                        p { class: "metric-value", "{positive_accounts}" }
                    }
                    article { class: "metric-card",
                        p { class: "metric-label", "Net Position" }
                        p { class: "{balance_tone}", "{total_balance}" }
                    }
                }

                section {
                    div { class: "section-header",
                        div {
                            h2 { class: "section-title", "Account balances" }
                        }
                    }

                    if balances.is_empty() {
                        div { class: "empty-state",
                            div { class: "empty-icon", "+" }
                            h3 { class: "section-title", "No accounts yet" }
                            p { class: "supporting-text", "Add a starter set or create your own." }
                            div { class: "status-actions",
                                Link {
                                    to: crate::Route::Setup {},
                                    class: "btn btn-primary",
                                    "Add starter accounts"
                                }
                                Link {
                                    to: crate::Route::NewAccount {},
                                    class: "btn btn-secondary",
                                    "New account"
                                }
                            }
                        }
                    } else {
                        table { class: "glass-card data-table stack-rows",
                            thead {
                                tr {
                                    th { "Code" }
                                    th { "Account" }
                                    th { class: "col-right", "Balance" }
                                }
                            }
                            tbody {
                                for balance in balances {
                                    tr {
                                        td { class: "mono muted m-lead",
                                            {balance.account_code.as_deref().unwrap_or("-")}
                                        }
                                        td { class: "m-title",
                                            div { class: "stack-sm",
                                                span { class: "label-strong", "{balance.account_name}" }
                                            }
                                        }
                                        td { class: "col-right mono m-value",
                                            span { class: if balance.balance >= Decimal::ZERO { "text-positive" } else { "text-negative" },
                                                "{balance.balance}"
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

/// Sends a new user to setup, which they can finish or skip.
#[component]
fn OpenSetup() -> Element {
    let nav = use_navigator();

    use_effect(move || {
        nav.replace(crate::Route::Setup {});
    });

    rsx! {
        div { class: "app-container",
            div { class: "message message-info", "Opening setup..." }
        }
    }
}

struct Step {
    title: &'static str,
    detail: &'static str,
    done: bool,
    /// False while an earlier step has to be done first.
    ready: bool,
    action: &'static str,
    to: crate::Route,
}

/// First steps for a new ledger, shown until they are all done or the user hides them.
#[component]
fn GettingStarted(progress: Progress) -> Element {
    let mut hidden = use_signal(|| progress.has(Milestone::Dismissed(Hint::GettingStarted)));
    let mut error = use_signal(|| None::<String>);

    let mut steps = vec![Step {
        title: "Set up your accounts",
        detail: "Pick a starter set or create your own",
        done: progress.has_accounts,
        ready: true,
        action: "Set up",
        to: crate::Route::Setup {},
    }];
    if let Some(provider) = progress.bank_sync_provider.clone() {
        steps.push(Step {
            title: "Connect your bank",
            detail: "Import transactions automatically",
            done: progress.bank_connected,
            ready: true,
            action: "Connect",
            to: crate::Route::BankConnect {
                source_id: None,
                provider: Some(provider),
            },
        });
    }
    steps.push(Step {
        title: "Record your first transaction",
        detail: if !progress.has_accounts {
            "Set up your accounts first"
        } else if progress.bank_sync_provider.is_some() {
            "Book one from your bank or enter it by hand"
        } else {
            "Enter a purchase, payment or transfer"
        },
        done: progress.has_transactions,
        // Transactions need accounts to post to
        ready: progress.has_accounts,
        action: "Add",
        to: crate::Route::NewTransaction {},
    });

    let total = steps.len();
    let done = steps.iter().filter(|step| step.done).count();
    if hidden() || done == total {
        return rsx! {};
    }

    let handle_hide = move |_| async move {
        hidden.set(true);
        error.set(None);
        if let Err(err) = record_milestone(Milestone::Dismissed(Hint::GettingStarted)).await {
            hidden.set(false);
            error.set(Some(format!("Couldn't hide the checklist. {err}")));
        }
    };

    rsx! {
        section {
            class: "section-card getting-started",
            aria_labelledby: "getting-started-title",
            div { class: "section-header",
                div {
                    h2 { id: "getting-started-title", class: "section-title", "Getting started" }
                    p { class: "supporting-text", "{done} of {total} done" }
                }
                button {
                    class: "btn btn-secondary btn-sm",
                    r#type: "button",
                    onclick: handle_hide,
                    "Hide"
                }
            }

            if let Some(err) = error() {
                div { class: "message message-error", role: "alert", "{err}" }
            }

            ol { class: "getting-started-steps",
                for (index , step) in steps.into_iter().enumerate() {
                    li { class: if step.done { "getting-started-step is-done" } else { "getting-started-step" },
                        span {
                            class: "getting-started-mark",
                            aria_hidden: "true",
                            if step.done {
                                "✓"
                            } else {
                                "{index + 1}"
                            }
                        }
                        div { class: "getting-started-copy",
                            span { class: "label-strong", "{step.title}" }
                            span { class: "supporting-text", "{step.detail}" }
                        }
                        if step.done {
                            span { class: "getting-started-done", "Done" }
                        } else if step.ready {
                            Link { class: "btn btn-primary btn-sm", to: step.to, "{step.action}" }
                        }
                    }
                }
            }
        }
    }
}
