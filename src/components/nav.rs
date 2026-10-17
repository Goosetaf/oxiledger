use crate::{
    models::user::CurrentUser,
    navigation::{Destination, NavPreferences},
    views::error::LoginRedirectNotice,
    Route,
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
                    r#"SELECT username, preferences -> 'navigation' AS "navigation?: serde_json::Value"
                       FROM users WHERE id = $1"#,
                    user_id
                )
                .fetch_optional(&pool)
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;

                return Ok(row.map(|r| CurrentUser {
                    username: r.username,
                    // Unreadable preferences fall back to the defaults
                    navigation: r
                        .navigation
                        .and_then(|stored| serde_json::from_value(stored).ok())
                        .unwrap_or_default(),
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

#[post("/api/preferences/navigation")]
async fn save_navigation(preferences: NavPreferences) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let navigation =
            serde_json::to_value(&preferences).map_err(|e| ServerFnError::new(e.to_string()))?;

        sqlx::query!(
            "UPDATE users
             SET preferences = jsonb_set(preferences, '{navigation}', $1), updated_at = NOW()
             WHERE id = $2",
            navigation,
            user_id
        )
        .execute(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    }

    Ok(())
}

/// The signed-in user's navigation preferences, shared by the app shell (which
/// renders them) and the Customize page (which edits them).
#[derive(Clone, Copy)]
pub struct NavSettings {
    preferences: Signal<NavPreferences>,
    save_error: Signal<Option<String>>,
    update: Callback<NavPreferences>,
}

impl NavSettings {
    pub fn preferences(&self) -> NavPreferences {
        (self.preferences)()
    }

    /// Why the latest change could not be saved, after it has been undone.
    pub fn save_error(&self) -> Option<String> {
        (self.save_error)()
    }

    /// Shows `next` right away and saves it in the background.
    ///
    /// Saves run one at a time and in order. Changes made while one is in
    /// flight collapse into a single follow-up save of the latest state. If
    /// the last save fails, the preferences return to what was last saved.
    pub fn update(&self, next: NavPreferences) {
        self.update.call(next);
    }
}

pub fn use_nav_settings() -> NavSettings {
    use_context()
}

/// Owns the navigation preferences for as long as the app shell is mounted, so
/// a save started on the Customize page finishes even after leaving it.
fn use_nav_settings_provider(initial: NavPreferences) -> NavSettings {
    let mut preferences = use_signal(|| initial.clone());
    // Last saved value; a failed save reverts to it
    let mut saved = use_signal(|| initial);
    let mut save_error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);
    let mut queued = use_signal(|| None::<NavPreferences>);

    let update = use_callback(move |next: NavPreferences| {
        preferences.set(next.clone());
        save_error.set(None);

        if saving() {
            // Picked up by the save in flight
            queued.set(Some(next));
            return;
        }

        saving.set(true);
        spawn(async move {
            let mut next = next;
            loop {
                match save_navigation(next.clone()).await {
                    Ok(()) => {
                        saved.set(next);
                        save_error.set(None);
                    }
                    Err(err) => save_error.set(Some(err.to_string())),
                }

                match queued.take() {
                    Some(newer) => next = newer,
                    None => break,
                }
            }

            // Revert anything the server did not accept
            if save_error.peek().is_some() {
                preferences.set(saved());
            }
            saving.set(false);
        });
    });

    use_context_provider(|| NavSettings {
        preferences,
        save_error,
        update,
    })
}

/// Closes the phone menu. Writes only when it is open, so the many clicks that
/// pass through here on a wide screen do not re-render the shell.
fn close_menu(mut menu_open: Signal<bool>) {
    if *menu_open.peek() {
        menu_open.set(false);
    }
}

/// Pages are titled after their destination unless they need something more
/// specific, so a new destination gets a title without being listed here.
fn topbar_title(route: &Route) -> &'static str {
    match route {
        Route::Dashboard {} => "Dashboard Overview",
        Route::BankAccountDetail { .. } => "Bank Account",
        Route::BankConnect { .. } => "Connect a Bank",
        Route::BankAccountMap { .. } => "Map Bank Accounts",
        Route::Customize {} => "Customize",
        _ => route
            .destination()
            .map_or("OxiLedger", |destination| destination.info().label),
    }
}

/// Layout for signed-in pages: checks the session, then hands over to [`Shell`].
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

/// The chrome around every signed-in page.
///
/// Wide screens get a sidebar listing every shown destination. Small screens
/// get a tab bar with the user's shortcuts, "+" and Menu; Menu opens that same
/// sidebar as a sheet, so the two can never list different things.
#[component]
fn Shell(user: CurrentUser) -> Element {
    let nav = use_navigator();
    let route = use_route::<Route>();
    let settings = use_nav_settings_provider(user.navigation.clone());
    let mut menu_open = use_signal(|| false);
    let mut menu_element = use_signal(|| None::<Rc<MountedData>>);
    let mut menu_button = use_signal(|| None::<Rc<MountedData>>);
    let mut menu_was_open = use_signal(|| false);

    // Navigating closes the menu
    use_effect(use_reactive!(|route| {
        let _ = route;
        close_menu(menu_open);
    }));

    // Move focus into the menu when it opens, and back to its tab when it closes
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

    let preferences = settings.preferences();
    let current = route.destination();
    let shortcuts = preferences.shortcuts();
    // Shortcuts split around "+"; Menu is last
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

            // Sidebar, shown as the Menu sheet on phones
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
                    for destination in preferences.shown().iter().copied() {
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
                span { class: "topbar-title", "{topbar_title(&route)}" }
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
