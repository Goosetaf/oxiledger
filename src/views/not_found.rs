use dioxus::prelude::*;

use crate::Route;

fn commit_not_found_status(path: &str) {
    #[cfg(feature = "server")]
    {
        dioxus::prelude::dioxus_fullstack::FullstackContext::commit_http_status(
            StatusCode::NOT_FOUND,
            Some(format!("No route matched {path}")),
        );
    }
}

#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let path = if segments.is_empty() {
        None
    } else {
        Some(format!("/{}", segments.join("/")))
    };

    commit_not_found_status(path.as_deref().unwrap_or("unknown path"));

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "message message-error",
                    div {
                        div { style: "font-size: 2rem; font-weight: 800; margin-bottom: 4px;",
                            "404"
                        }
                        div { style: "font-weight: 600; margin-bottom: 8px;", "Page not found" }
                        if let Some(path) = path {
                            div { style: "opacity: 0.7; font-size: 0.875rem; margin-bottom: 16px;",
                                "The path \"{path}\" doesn't exist."
                            }
                        } else {
                            div { style: "opacity: 0.7; font-size: 0.875rem; margin-bottom: 16px;",
                                "The page you requested doesn't exist."
                            }
                        }
                        Link {
                            to: Route::Dashboard {},
                            style: "text-decoration: underline; font-size: 0.875rem;",
                            "Go back to the dashboard"
                        }
                    }
                }
            }
        }
    }
}
