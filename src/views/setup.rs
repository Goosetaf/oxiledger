use crate::components::button::Button;
use crate::views::error::Redirect;
use dioxus::prelude::*;

use crate::models::user::{AuthOptions, RegisterRequest};

#[cfg(feature = "server")]
use tower_cookies::Cookie;

/// Creates the first account on an instance with no users, and signs it in.
#[post("/api/auth/setup")]
async fn setup_action(req: RegisterRequest) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth;
        use std::sync::Arc;

        let (pool, cookies) = auth::extract_context().await?;

        // With password login disabled, the first account comes from SSO instead.
        {
            use dioxus::prelude::dioxus_fullstack::FullstackContext;
            if let Some(ctx) = FullstackContext::current() {
                if let Some(config) = ctx.extension::<Arc<crate::server::oidc::OidcConfig>>() {
                    if config.disable_password_login {
                        return Err(ServerFnError::new(
                            "Password login is disabled. Please use SSO.",
                        ));
                    }
                }
            }
        }

        if req.password.len() < 8 {
            return Err(ServerFnError::new("Password must be at least 8 characters"));
        }

        let hash =
            auth::hash_password(&req.password).map_err(|e| ServerFnError::new(e.to_string()))?;

        let mut tx = auth::begin_setup(&pool)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?
            .ok_or_else(|| ServerFnError::new("This instance is already set up"))?;

        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
            req.username,
            req.email,
            hash
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        let token = auth::create_session(&pool, user_id)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        let mut cookie = Cookie::new(auth::SESSION_COOKIE, token);
        cookie.set_http_only(true);
        cookie.set_path("/");
        cookie.set_same_site(tower_cookies::cookie::SameSite::Strict);
        cookies.add(cookie);
    }

    Ok(())
}

#[component]
pub fn Setup() -> Element {
    let options_future = use_server_future(crate::views::login::get_auth_options)?;

    let AuthOptions {
        providers,
        password_login: password_enabled,
        needs_setup,
        ..
    } = match options_future() {
        Some(Ok(v)) => v,
        Some(Err(e)) => return Err(e.into()),
        None => {
            return rsx! {
                div { class: "auth-shell",
                    div { class: "auth-shell-inner",
                        div { class: "message message-info", "Loading..." }
                    }
                }
            };
        }
    };

    // Once the first account exists, setup is over.
    if !needs_setup {
        return rsx! {
            Redirect { to: crate::Route::Login {} }
        };
    }

    let mut username = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error_msg = use_signal(|| None::<String>);
    let mut submitting = use_signal(|| false);
    let nav = use_navigator();

    let handle_submit = move |e: Event<FormData>| {
        e.prevent_default();
        async move {
            submitting.set(true);
            error_msg.set(None);
            let req = RegisterRequest {
                username: username(),
                email: email(),
                password: password(),
            };

            match setup_action(req).await {
                Ok(()) => {
                    let _ = nav.push(crate::Route::Dashboard {});
                }
                Err(e) => {
                    error_msg.set(Some(e.to_string()));
                }
            }

            submitting.set(false);
        }
    };

    rsx! {
        div { class: "auth-shell",
            div { class: "auth-shell-inner",
                div { class: "auth-panel auth-panel-single",
                    div { class: "auth-panel-header",
                        div { class: "auth-brand",
                            div { class: "auth-brand-mark",
                                img {
                                    src: asset!("/assets/header.svg"),
                                    alt: "OxiLedger logo",
                                }
                            }
                            div { class: "brand-copy",
                                span { class: "brand-title", "OxiLedger" }
                                span { class: "brand-subtitle", "Set up this instance" }
                            }
                        }
                    }

                    section { class: "auth-panel-body",
                        div { class: "auth-copy",
                            span { class: "eyebrow", "Setup" }
                            h2 { class: "section-title", "Create the first account" }
                            p { class: "supporting-text",
                                "This instance has no accounts yet. Create the first one to start using it."
                            }
                        }

                        if let Some(err) = error_msg() {
                            div { class: "message message-error", "{err}" }
                        }

                        // Signing in through a provider creates the first account from it
                        if !providers.is_empty() {
                            div { class: "auth-stack",
                                for provider in &providers {
                                    a {
                                        href: format!("/auth/oidc/{}/start", provider.id),
                                        class: "btn btn-secondary btn-block",
                                        "Continue with {provider.name}"
                                    }
                                }
                            }
                        }

                        // Divider only when both methods are visible
                        if password_enabled && !providers.is_empty() {
                            div { class: "auth-divider",
                                span { "or" }
                            }
                        }

                        if password_enabled {
                            form { class: "auth-stack", onsubmit: handle_submit,
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "setup-username",
                                        "Username"
                                    }
                                    input {
                                        id: "setup-username",
                                        class: "input",
                                        r#type: "text",
                                        placeholder: "Choose a username",
                                        value: username,
                                        oninput: move |e| username.set(e.value()),
                                        required: true,
                                    }
                                }

                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "setup-email",
                                        "Email"
                                    }
                                    input {
                                        id: "setup-email",
                                        class: "input",
                                        r#type: "email",
                                        placeholder: "name@company.com",
                                        value: email,
                                        oninput: move |e| email.set(e.value()),
                                        required: true,
                                    }
                                }

                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "setup-password",
                                        "Password"
                                    }
                                    input {
                                        id: "setup-password",
                                        class: "input",
                                        r#type: "password",
                                        placeholder: "At least 8 characters",
                                        value: password,
                                        oninput: move |e| password.set(e.value()),
                                        required: true,
                                        minlength: 8,
                                    }
                                    span { class: "field-note", "Use at least 8 characters." }
                                }

                                Button {
                                    class: "btn btn-primary btn-block".to_string(),
                                    r#type: "submit",
                                    disabled: submitting(),
                                    if submitting() {
                                        "Creating account..."
                                    } else {
                                        "Create account"
                                    }
                                }
                            }
                        }

                        if !password_enabled && providers.is_empty() {
                            div { class: "message message-info",
                                "No way to create an account is configured. Enable password login or add an OIDC provider."
                            }
                        }
                    }
                }
            }
        }
    }
}
