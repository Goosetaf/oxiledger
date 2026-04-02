use crate::components::button::Button;
use dioxus::prelude::*;

use crate::models::user::LoginRequest;

#[cfg(feature = "server")]
use {
    sqlx::PgPool,
    tower_cookies::{Cookie, Cookies},
};

#[post("/api/auth/login")]
async fn login_action(req: LoginRequest) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth;
        use std::sync::Arc;

        let (pool, cookies) = auth::extract_context().await?;

        // Honour the DISABLE_PASSWORD_LOGIN flag even at the API layer so that
        // a direct POST to this endpoint cannot bypass it.
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

        let row = sqlx::query!(
            "SELECT id, password_hash FROM users WHERE username = $1",
            req.username
        )
        .fetch_optional(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?
        .ok_or_else(|| ServerFnError::new("Invalid username or password"))?;

        let hash = row
            .password_hash
            .ok_or_else(|| ServerFnError::new("This account uses OIDC login"))?;

        let valid = auth::verify_password(&req.password, &hash)
            .map_err(|e| ServerFnError::new(e.to_string()))?;

        if !valid {
            return Err(ServerFnError::new("Invalid username or password"));
        }

        let token = auth::create_session(&pool, row.id)
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
pub fn Login() -> Element {
    // Fetch OIDC config so we can show provider buttons and conditionally hide
    // the password form.
    let config_future = use_server_future(crate::views::oidc::get_oidc_config)?;

    let (providers, password_enabled) = match config_future() {
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

    let mut username = use_signal(String::new);
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
            let req = LoginRequest {
                username: username(),
                password: password(),
            };

            match login_action(req).await {
                Ok(()) => {
                    nav.push(crate::Route::Dashboard {});
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
                                span { class: "brand-subtitle", "Sign in to continue" }
                            }
                        }
                    }

                    section { class: "auth-panel-body",
                        div { class: "auth-copy",
                            span { class: "eyebrow", "Welcome Back" }
                            h2 { class: "section-title", "Sign in to your workspace" }
                            if password_enabled {
                                p { class: "supporting-text",
                                    "Use your local account credentials to continue."
                                }
                            } else {
                                p { class: "supporting-text",
                                    "Use your organisation's single sign-on to continue."
                                }
                            }
                        }

                        if let Some(err) = error_msg() {
                            div { class: "message message-error", "{err}" }
                        }

                        // OIDC provider buttons (shown when providers exist)
                        if !providers.is_empty() {
                            div { class: "auth-stack",
                                for provider in &providers {
                                    a {
                                        href: format!("/auth/oidc/{}/start", provider.id),
                                        class: "btn btn-secondary btn-block",
                                        "Sign in with {provider.name}"
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

                        // Classic username + password form
                        if password_enabled {
                            form { class: "auth-stack", onsubmit: handle_submit,
                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "username",
                                        "Username"
                                    }
                                    input {
                                        id: "username",
                                        class: "input",
                                        r#type: "text",
                                        placeholder: "Enter your username",
                                        value: username,
                                        oninput: move |e| username.set(e.value()),
                                        required: true,
                                    }
                                }

                                div { class: "field-block",
                                    label {
                                        class: "field-label",
                                        r#for: "password",
                                        "Password"
                                    }
                                    input {
                                        id: "password",
                                        class: "input",
                                        r#type: "password",
                                        placeholder: "Enter your password",
                                        value: password,
                                        oninput: move |e| password.set(e.value()),
                                        required: true,
                                    }
                                }

                                Button {
                                    class: "btn btn-primary btn-block".to_string(),
                                    r#type: "submit",
                                    disabled: submitting(),
                                    if submitting() {
                                        "Signing in..."
                                    } else {
                                        "Sign in"
                                    }
                                }
                            }

                            p { class: "auth-footnote",
                                "Need access? "
                                Link {
                                    to: crate::Route::Register {},
                                    class: "auth-link",
                                    "Create an account"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
