use dioxus::prelude::*;

#[component]
pub fn OidcLogin() -> Element {
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
                                span { class: "brand-subtitle", "OIDC status" }
                            }
                        }
                    }

                    div { class: "auth-panel-body",
                        div { class: "auth-copy",
                            span { class: "eyebrow", "OIDC Login" }
                            h2 { class: "section-title", "Authentication provider not configured" }
                            p { class: "supporting-text",
                                "Use the local account flow for now while external identity integration is completed."
                            }
                        }

                        div { class: "message message-info",
                            "OIDC authentication is not yet available in this build."
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
    }
}

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
                                span { class: "brand-subtitle", "Callback status" }
                            }
                        }
                    }

                    div { class: "auth-panel-body",
                        div { class: "auth-copy",
                            span { class: "eyebrow", "OIDC Callback" }
                            h2 { class: "section-title", "Awaiting provider integration" }
                            p { class: "supporting-text",
                                "No token exchange is configured yet, so this route currently serves as a polished placeholder."
                            }
                        }

                        div { class: "message message-info",
                            "OIDC callback handling is not configured in this build."
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
    }
}
