use crate::views::error::LoginRedirectNotice;
use dioxus::prelude::*;

#[cfg(feature = "server")]
use {sqlx::PgPool, tower_cookies::Cookies};

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

fn active_nav_class(route: &crate::Route, item: &str) -> &'static str {
    match (route, item) {
        (crate::Route::Dashboard {}, "dashboard") => "nav-link is-active",
        (crate::Route::Accounts {}, "accounts")
        | (crate::Route::NewAccount {}, "accounts")
        | (crate::Route::EditAccount { .. }, "accounts") => "nav-link is-active",
        (crate::Route::Transactions {}, "transactions")
        | (crate::Route::NewTransaction {}, "transactions")
        | (crate::Route::EditTransaction { .. }, "transactions") => "nav-link is-active",
        _ => "nav-link",
    }
}

fn topbar_title(route: &crate::Route) -> &'static str {
    match route {
        crate::Route::Dashboard {} => "Dashboard Overview",
        crate::Route::Accounts {}
        | crate::Route::NewAccount {}
        | crate::Route::EditAccount { .. } => "Accounts",
        crate::Route::Transactions {}
        | crate::Route::NewTransaction {}
        | crate::Route::EditTransaction { .. } => "Transactions",
        _ => "OxiLedger",
    }
}

#[component]
pub fn Navbar() -> Element {
    let user_future = use_server_future(get_current_user)?;
    let nav = use_navigator();
    let route = use_route::<crate::Route>();

    let handle_logout = move |_| {
        let nav = nav.clone();
        async move {
            let _ = logout_action().await;
            nav.push(crate::Route::Login {});
        }
    };

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

            // Fixed top bar
            header { class: "topbar",
                span { class: "topbar-title", "{topbar_title(&route)}" }
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
        }
    }
}
