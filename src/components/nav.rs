use crate::views::error::LoginRedirectNotice;
use dioxus::prelude::*;

#[get("/api/auth/me")]
async fn get_current_user() -> Result<Option<String>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        match require_auth(&pool, &cookies).await {
            Ok(user_id) => {
                let row = sqlx::query!("SELECT username FROM users WHERE id = $1", user_id)
                    .fetch_optional(&pool)
                    .await
                    .map_err(|e| ServerFnError::new(e.to_string()))?;
                return Ok(row.map(|r| r.username));
            }
            Err(ServerFnError::ServerError { code, .. })
                if code == StatusCode::UNAUTHORIZED.as_u16() =>
            {
                return Ok(None);
            }
            Err(err) => return Err(err),
        }
    }

    #[cfg(not(feature = "server"))]
    Ok(None)
}

#[post("/api/auth/logout")]
async fn logout_action() -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, SESSION_COOKIE};
        use tower_cookies::Cookie;

        let (pool, cookies) = extract_context().await?;

        if let Some(cookie) = cookies.get(SESSION_COOKIE) {
            let token = cookie.value().to_string();
            let _ = crate::server::auth::delete_session(&pool, &token).await;
        }

        let mut removal = Cookie::new(SESSION_COOKIE, "");
        removal.set_path("/");
        cookies.remove(removal);
    }

    Ok(())
}

fn is_nav_item_active(route: &crate::Route, item: &str) -> bool {
    matches!(
        (route, item),
        (crate::Route::Dashboard {}, "dashboard")
            | (crate::Route::Accounts {}, "accounts")
            | (crate::Route::NewAccount {}, "accounts")
            | (crate::Route::EditAccount { .. }, "accounts")
            | (crate::Route::Transactions {}, "transactions")
            | (crate::Route::NewTransaction {}, "transactions")
            | (crate::Route::EditTransaction { .. }, "transactions")
            | (crate::Route::NewTransactionFromBank { .. }, "transactions")
            | (crate::Route::BankAccounts {}, "bank_accounts")
            | (crate::Route::NewBankAccount {}, "bank_accounts")
            | (crate::Route::BankAccountDetail { .. }, "bank_accounts")
            | (crate::Route::BankAccountEdit { .. }, "bank_accounts")
            | (crate::Route::BankConnect { .. }, "bank_accounts")
            | (crate::Route::BankAccountMap { .. }, "bank_accounts")
    )
}

fn active_nav_class(route: &crate::Route, item: &str) -> &'static str {
    if is_nav_item_active(route, item) {
        "nav-link is-active"
    } else {
        "nav-link"
    }
}

fn active_tab_class(route: &crate::Route, item: &str) -> &'static str {
    if is_nav_item_active(route, item) {
        "tab is-active"
    } else {
        "tab"
    }
}

/// Outline icon for the mobile tab bar. `d` is the path data on a 24x24 grid.
#[component]
fn TabIcon(d: String) -> Element {
    rsx! {
        svg {
            class: "tab-icon",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.75",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            path { d }
        }
    }
}

const ICON_DASHBOARD: &str = "M4 5h7v7H4zM13 5h7v4h-7zM13 12h7v7h-7zM4 15h7v4H4z";
const ICON_TRANSACTIONS: &str = "M6 8h12M15 5l3 3-3 3M18 16H6M9 13l-3 3 3 3";
const ICON_ACCOUNTS: &str = "M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01";
const ICON_BANKS: &str = "M3 10l9-6 9 6M5 10v8M9.7 10v8M14.3 10v8M19 10v8M3 20h18";
const ICON_ADD: &str = "M12 5v14M5 12h14";

fn topbar_title(route: &crate::Route) -> &'static str {
    match route {
        crate::Route::Dashboard {} => "Dashboard Overview",
        crate::Route::Accounts {}
        | crate::Route::NewAccount {}
        | crate::Route::EditAccount { .. } => "Accounts",
        crate::Route::Transactions {}
        | crate::Route::NewTransaction {}
        | crate::Route::NewTransactionFromBank { .. }
        | crate::Route::EditTransaction { .. } => "Transactions",
        crate::Route::BankAccounts {} => "Bank Accounts",
        crate::Route::BankAccountDetail { .. } => "Bank Account",
        crate::Route::BankConnect { .. } => "Connect a Bank",
        crate::Route::BankAccountMap { .. } => "Map Bank Accounts",
        _ => "OxiLedger",
    }
}

#[component]
pub fn Navbar() -> Element {
    let user_future = use_server_future(get_current_user)?;
    let nav = use_navigator();
    let route = use_route::<crate::Route>();

    // A `Callback` is `Copy`, so the sidebar and the mobile top bar can share it.
    let handle_logout = use_callback(move |_: MouseEvent| {
        let nav = nav.clone();
        spawn(async move {
            let _ = logout_action().await;
            nav.push(crate::Route::Login {});
        });
    });

    let username = match user_future() {
        Some(Ok(Some(name))) => name,
        Some(Ok(None)) => {
            return rsx! {
                LoginRedirectNotice {
                    title: "Please sign in".to_string(),
                    message: "You need an active session to view this page.".to_string(),
                }
            };
        }
        Some(Err(err)) => return Err(err.into()),
        None => {
            return rsx! {
                div { class: "app-container",
                    div { class: "message message-info", "Checking your session..." }
                }
            };
        }
    };

    rsx! {
        div { class: "app-shell",
            // Sidebar
            nav { class: "sidebar",
                // Brand
                div { class: "sidebar-brand",
                    span { class: "brand-title", "OxiLedger" }
                }

                // Nav links
                div { class: "sidebar-nav",
                    Link {
                        to: crate::Route::Dashboard {},
                        class: active_nav_class(&route, "dashboard"),
                        "Dashboard"
                    }
                    Link {
                        to: crate::Route::Transactions {},
                        class: active_nav_class(&route, "transactions"),
                        "Transactions"
                    }
                    Link {
                        to: crate::Route::Accounts {},
                        class: active_nav_class(&route, "accounts"),
                        "Accounts"
                    }
                    Link {
                        to: crate::Route::BankAccounts {},
                        class: active_nav_class(&route, "bank_accounts"),
                        "Bank Accounts"
                    }
                }

                // Footer links
                div { class: "sidebar-footer",
                    div {
                        class: "nav-user",
                        style: "padding: 8px 24px; align-items: flex-start;",
                        span { class: "eyebrow", "Signed In" }
                        span { class: "label-strong", "{username}" }
                    }
                    button {
                        class: "nav-link",
                        style: "width: 100%; justify-content: flex-start; border-radius: 0; text-align: left; background: none; cursor: pointer;",
                        onclick: handle_logout,
                        "Sign out"
                    }
                }
            }

            // Fixed top bar (on small screens it carries the brand and sign-out,
            // since the sidebar is hidden there)
            header { class: "topbar",
                span { class: "topbar-title", "{topbar_title(&route)}" }
                span { class: "topbar-brand brand-title", "OxiLedger" }
                div { class: "topbar-account",
                    span { class: "topbar-user", "{username}" }
                    button {
                        class: "topbar-signout",
                        r#type: "button",
                        onclick: handle_logout,
                        "Sign out"
                    }
                }
            }

            // Main content
            main { class: "shell-inner",
                SuspenseBoundary {
                    fallback: |_| rsx! {
                        div { class: "app-container",
                            div { class: "message message-info", "Loading page..." }
                        }
                    },
                    Outlet::<crate::Route> {}
                }
            }

            // Bottom tab bar, shown instead of the sidebar on small screens
            nav { class: "tabbar", aria_label: "Primary",
                Link {
                    to: crate::Route::Dashboard {},
                    class: active_tab_class(&route, "dashboard"),
                    TabIcon { d: ICON_DASHBOARD }
                    span { class: "tab-label", "Dashboard" }
                }
                Link {
                    to: crate::Route::Transactions {},
                    class: active_tab_class(&route, "transactions"),
                    TabIcon { d: ICON_TRANSACTIONS }
                    span { class: "tab-label", "Transactions" }
                }
                Link {
                    to: crate::Route::NewTransaction {},
                    class: "tab-add",
                    aria_label: "New transaction",
                    TabIcon { d: ICON_ADD }
                }
                Link {
                    to: crate::Route::Accounts {},
                    class: active_tab_class(&route, "accounts"),
                    TabIcon { d: ICON_ACCOUNTS }
                    span { class: "tab-label", "Accounts" }
                }
                Link {
                    to: crate::Route::BankAccounts {},
                    class: active_tab_class(&route, "bank_accounts"),
                    TabIcon { d: ICON_BANKS }
                    span { class: "tab-label", "Banks" }
                }
            }
        }
    }
}
