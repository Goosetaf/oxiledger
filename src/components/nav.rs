use crate::{
    models::user::CurrentUser, navigation::Destination, preferences::use_preferences_provider,
    views::error::LoginRedirectNotice, Route,
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::ld_icons::{LdMenu, LdPlus},
    Icon,
};
use std::rc::Rc;

#[get("/api/auth/me")]
async fn get_current_user() -> Result<Option<CurrentUser>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        match require_auth(&pool, &cookies).await {
            Ok(user_id) => {
                let row = sqlx::query!(
                    "SELECT username, preferences FROM users WHERE id = $1",
                    user_id
                )
                .fetch_optional(&pool)
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;

                return Ok(row.map(|r| CurrentUser {
                    username: r.username,
                    preferences: serde_json::from_value(r.preferences).unwrap_or_default(),
                }));
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

fn close_menu(mut menu_open: Signal<bool>) {
    if *menu_open.peek() {
        menu_open.set(false);
    }
}

#[component]
pub fn Navbar() -> Element {
    let user_future = use_server_future(get_current_user)?;

    match user_future() {
        Some(Ok(Some(user))) => rsx! {
            Shell { user }
        },
        Some(Ok(None)) => rsx! {
            LoginRedirectNotice {
                title: "Please sign in".to_string(),
                message: "You need an active session to view this page.".to_string(),
            }
        },
        Some(Err(err)) => Err(err.into()),
        None => rsx! {
            div { class: "app-container",
                div { class: "message message-info", "Checking your session..." }
            }
        },
    }
}

/// The sidebar, tab bar and menu around every signed-in page.
#[component]
fn Shell(user: CurrentUser) -> Element {
    let nav = use_navigator();
    let route = use_route::<Route>();
    let preferences = use_preferences_provider(user.preferences.clone());
    let mut menu_open = use_signal(|| false);
    let mut menu_element = use_signal(|| None::<Rc<MountedData>>);
    let mut menu_button = use_signal(|| None::<Rc<MountedData>>);
    let mut menu_was_open = use_signal(|| false);

    use_effect(use_reactive!(|route| {
        let _ = route;
        close_menu(menu_open);
    }));

    use_effect(move || {
        let open = menu_open();
        if open == *menu_was_open.peek() {
            return;
        }
        menu_was_open.set(open);

        let target = if open {
            menu_element.peek().clone()
        } else {
            menu_button.peek().clone()
        };
        if let Some(target) = target {
            spawn(async move {
                let _ = target.set_focus(true).await;
            });
        }
    });

    let handle_logout = move |_| async move {
        let _ = logout_action().await;
        nav.push(Route::Login {});
    };

    let navigation = preferences.get().navigation;
    let current = route.destination();
    let shortcuts = navigation.shortcuts();
    let (before_add, after_add) = shortcuts.split_at(shortcuts.len().div_ceil(2));
    let current_is_in_menu = !current.is_some_and(|destination| shortcuts.contains(&destination));

    rsx! {
        div {
            class: if menu_open() { "app-shell menu-open" } else { "app-shell" },
            onkeydown: move |event| {
                if event.key() == Key::Escape {
                    close_menu(menu_open);
                }
            },

            nav {
                id: "primary-navigation",
                class: "sidebar",
                aria_label: "Main",
                tabindex: "-1",
                onmounted: move |event| menu_element.set(Some(event.data())),
                onclick: move |_| close_menu(menu_open),

                // Brand
                div { class: "sidebar-brand",
                    span { class: "brand-title", "OxiLedger" }
                }

                // Nav links
                div { class: "sidebar-nav",
                    for destination in navigation.shown().iter().copied() {
                        Link {
                            key: "{destination:?}",
                            to: destination.info().route,
                            class: if current == Some(destination) { "nav-link is-active" } else { "nav-link" },
                            "{destination.info().label}"
                        }
                    }
                }

                // Footer
                div { class: "sidebar-footer",
                    Link {
                        to: Route::Customize {},
                        class: if matches!(route, Route::Customize {}) { "nav-link is-active" } else { "nav-link" },
                        "Customize"
                    }
                    div { class: "sidebar-account",
                        div { class: "account-identity",
                            span { class: "eyebrow", "Signed In" }
                            span { class: "account-name", "{user.username}" }
                        }
                        button {
                            class: "account-signout",
                            r#type: "button",
                            onclick: handle_logout,
                            "Sign out"
                        }
                    }
                }
            }

            if menu_open() {
                div {
                    class: "menu-backdrop",
                    onclick: move |_| close_menu(menu_open),
                }
            }

            // Fixed top bar
            header { class: "topbar",
                span { class: "topbar-title", "{route.title()}" }
                span { class: "topbar-brand brand-title", "OxiLedger" }
            }

            // Main content
            main { class: "shell-inner", "inert": if menu_open() { "true" },
                SuspenseBoundary {
                    fallback: |_| rsx! {
                        div { class: "app-container",
                            div { class: "message message-info", "Loading page..." }
                        }
                    },
                    Outlet::<Route> {}
                }
            }

            // Bottom tab bar for phones
            nav {
                class: "tabbar",
                aria_label: "Shortcuts",
                onclick: move |_| close_menu(menu_open),
                div { class: "tabbar-side",
                    for destination in before_add.iter().copied() {
                        TabLink {
                            key: "{destination:?}",
                            destination,
                            active: current == Some(destination),
                        }
                    }
                }
                Link {
                    to: Route::NewTransaction {},
                    class: "tab-add",
                    aria_label: "New transaction",
                    Icon { class: "icon", icon: LdPlus }
                }
                div { class: "tabbar-side",
                    for destination in after_add.iter().copied() {
                        TabLink {
                            key: "{destination:?}",
                            destination,
                            active: current == Some(destination),
                        }
                    }
                    button {
                        class: if menu_open() || current_is_in_menu { "tab is-active" } else { "tab" },
                        r#type: "button",
                        "aria-expanded": "{menu_open}",
                        "aria-controls": "primary-navigation",
                        onmounted: move |event| menu_button.set(Some(event.data())),
                        onclick: move |event| {
                            // Don't let the tab bar's handler close it again
                            event.stop_propagation();
                            menu_open.toggle();
                        },
                        Icon { class: "icon", icon: LdMenu }
                        span { class: "tab-label", "Menu" }
                    }
                }
            }
        }
    }
}

#[component]
fn TabLink(destination: Destination, active: bool) -> Element {
    let info = destination.info();

    rsx! {
        Link { to: info.route, class: if active { "tab is-active" } else { "tab" },
            {(info.icon)()}
            span { class: "tab-label", "{info.short_label}" }
        }
    }
}
