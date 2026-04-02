use crate::models::oidc::OidcProviderInfo;
use dioxus::prelude::*;

#[cfg(feature = "server")]
use std::sync::Arc;

/// Returns the list of configured OIDC providers and whether password login is enabled.
/// This is the only server function needed in this module — the actual OAuth
/// redirect and callback are handled by native axum routes, not server functions.
#[get("/api/auth/oidc/config")]
pub async fn get_oidc_config() -> Result<(Vec<OidcProviderInfo>, bool), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use dioxus::prelude::dioxus_fullstack::FullstackContext;

        let ctx =
            FullstackContext::current().ok_or_else(|| ServerFnError::new("No request context"))?;

        let config = ctx
            .extension::<Arc<crate::server::oidc::OidcConfig>>()
            .ok_or_else(|| ServerFnError::new("OIDC config not available"))?;

        return Ok((config.provider_infos(), !config.disable_password_login));
    }

    #[cfg(not(feature = "server"))]
    Ok((vec![], true))
}

/// The OIDC login selection page.
///
/// Renders one "Sign in with <name>" button per configured provider.
/// When `DISABLE_PASSWORD_LOGIN=true` the link back to classic login is hidden.
#[component]
pub fn OidcLogin() -> Element {
    let config_future = use_server_future(get_oidc_config)?;

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
