use crate::models::transaction::AccountBalance;
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
                        p { class: "section-subtitle",
                            "Overview of your account balances and metrics."
                        }
                    }
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
                            p { class: "supporting-text",
                                "Create your first account to start seeing balances here."
                            }
                            Link {
                                to: crate::Route::Accounts {},
                                class: "btn btn-primary",
                                "Create an account"
                            }
                        }
                    } else {
                        table { class: "glass-card data-table",
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
                                        td { class: "mono muted",
                                            {balance.account_code.as_deref().unwrap_or("-")}
                                        }
                                        td {
                                            div { class: "stack-sm",
                                                span { class: "label-strong", "{balance.account_name}" }
                                            }
                                        }
                                        td { class: "col-right mono",
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
