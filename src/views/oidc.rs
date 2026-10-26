use crate::{models::user::AuthOptions, views::login::get_auth_options};
use dioxus::prelude::*;

/// The OIDC login selection page.
///
/// Renders one "Sign in with <name>" button per configured provider.
/// When `DISABLE_PASSWORD_LOGIN=true` the link back to classic login is hidden.
#[component]
pub fn OidcLogin() -> Element {
    let options_future = use_server_future(get_auth_options)?;

    let AuthOptions {
        providers,
        password_login: password_enabled,
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

    rsx! {
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
                                span { class: "brand-subtitle", "Single sign-on" }
                            }
                        }
                    }

                    div { class: "auth-panel-body",
                        div { class: "auth-copy",
                            span { class: "eyebrow", "Single Sign-On" }
                            h2 { class: "section-title", "Sign in with your organisation" }
                            p { class: "supporting-text",
                                "Choose your identity provider below to continue."
                            }
                        }

                        if providers.is_empty() {
                            div { class: "message message-info",
                                "No identity providers are configured. Contact your administrator."
                            }
                        } else {
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

                        if password_enabled {
                            p { class: "auth-footnote",
                                "Or "
                                Link {
                                    to: crate::Route::Login {},
                                    class: "auth-link",
                                    "sign in with your local account"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Transitional page shown while the OIDC callback is being processed.
///
/// In practice the browser is redirected server-side directly to `/` after a
/// successful callback, so users should only see this page briefly or if
/// something went wrong before the axum handler ran.
#[component]
pub fn OidcCallback() -> Element {
    rsx! {
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
                                span { class: "brand-subtitle", "Completing sign-in" }
                            }
                        }
                    }

                    div { class: "auth-panel-body",
                        div { class: "auth-copy",
                            span { class: "eyebrow", "Redirecting" }
                            h2 { class: "section-title", "Completing sign-in..." }
                            p { class: "supporting-text",
                                "Please wait while we verify your identity and create your session."
                            }
                        }

                        div { class: "message message-info", "Finalising authentication..." }

                        Link {
                            to: crate::Route::Login {},
                            class: "btn btn-secondary",
                            "Back to sign in"
                        }
                    }
                }
            }
        }
    }
}
