use crate::components::button::Button;
use dioxus::prelude::*;

use crate::models::user::RegisterRequest;

#[cfg(feature = "server")]
use {
    axum::Extension,
    sqlx::PgPool,
    tower_cookies::{Cookie, Cookies},
};

#[post("/api/auth/register")]
async fn register_action(req: RegisterRequest) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth;
        use std::sync::Arc;

        let (pool, cookies) = auth::extract_context().await?;

        // Block registration when password login is disabled.
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

        let exists = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM users WHERE username = $1 OR email = $2)",
            req.username,
            req.email
        )
        .fetch_one(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .unwrap_or(false);

        if exists {
            return Err(ServerFnError::new("Username or email is already taken"));
        }

        if req.password.len() < 8 {
            return Err(ServerFnError::new("Password must be at least 8 characters"));
        }

        let hash =
            auth::hash_password(&req.password).map_err(|e| ServerFnError::new(e.to_string()))?;

        let user_id = sqlx::query_scalar!(
            "INSERT INTO users (username, email, password_hash) VALUES ($1, $2, $3) RETURNING id",
            req.username,
            req.email,
            hash
        )
        .fetch_one(&pool)
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
pub fn Register() -> Element {
    // Check whether password login is enabled before rendering the form.
    let config_future = use_server_future(crate::views::oidc::get_oidc_config)?;

    let (_providers, password_enabled) = match config_future() {
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

    // If password login is disabled, redirect to the SSO page.
    if !password_enabled {
        return rsx! {
            div { class: "auth-shell",
                div { class: "auth-shell-inner",
                    section { class: "auth-panel auth-panel-single",
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
                                    span { class: "brand-subtitle", "Registration unavailable" }
                                }
                            }
                        }
                        div { class: "auth-panel-body",
                            div { class: "auth-copy",
                                h2 { class: "section-title", "Registration is disabled" }
                                p { class: "supporting-text",
                                    "This instance requires single sign-on. Contact your administrator to get access."
                                }
                            }
                            Link {
                                to: crate::Route::Login {},
                                class: "btn btn-primary",
                                "Back to sign in"
                            }
                        }
                    }
                }
            }
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
        let nav = nav.clone();
        async move {
            submitting.set(true);
            error_msg.set(None);
            let req = RegisterRequest {
                username: username(),
                email: email(),
                password: password(),
            };

            match register_action(req).await {
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
                                span { class: "brand-subtitle", "Create your workspace" }
                            }
                        }
                    }

                    section { class: "auth-panel-body",
                        div { class: "auth-copy",
                            span { class: "eyebrow", "Create Account" }
                            h2 { class: "section-title", "Open your OxiLedger workspace" }
                            p { class: "supporting-text",
                                "Set up a local account to begin managing your books."
                            }
                        }

                        if let Some(err) = error_msg() {
                            div { class: "message message-error", "{err}" }
                        }

                        form { class: "auth-stack", onsubmit: handle_submit,
                            div { class: "field-block",
                                label {
                                    class: "field-label",
                                    r#for: "register-username",
                                    "Username"
                                }
                                input {
                                    id: "register-username",
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
                                    r#for: "register-email",
                                    "Email"
                                }
                                input {
                                    id: "register-email",
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
                                    r#for: "register-password",
                                    "Password"
                                }
                                input {
                                    id: "register-password",
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

                        p { class: "auth-footnote",
                            "Already have an account? "
                            Link {
                                to: crate::Route::Login {},
                                class: "auth-link",
                                "Sign in"
                            }
                        }
                    }
                }
            }
        }
    }
}
