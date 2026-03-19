use dioxus::prelude::*;
use dioxus::CapturedError;
use dioxus_router::ParseRouteError;

use crate::Route;

fn commit_login_redirect() {
    #[cfg(feature = "server")]
    {
        use dioxus::prelude::dioxus_fullstack::{
            http::header::LOCATION, FullstackContext, HeaderValue,
        };

        if let Some(ctx) = FullstackContext::current() {
            ctx.add_response_header(LOCATION, HeaderValue::from_static("/login"));
        }

        FullstackContext::commit_http_status(
            StatusCode::FOUND,
            Some("Redirecting to /login".to_string()),
        );
    }
}

fn commit_status(status: StatusCode, message: String) {
    #[cfg(feature = "server")]
    {
        dioxus::prelude::dioxus_fullstack::FullstackContext::commit_http_status(
            status,
            Some(message),
        );
    }
}

fn describe_error(error: &CapturedError) -> (StatusCode, String, String, Option<String>) {
    if error.downcast_ref::<ParseRouteError>().is_some() {
        return (
            StatusCode::NOT_FOUND,
            "Page not found".to_string(),
            "The page you requested does not exist or may have moved.".to_string(),
            None,
        );
    }

    let err_text = error.to_string();
    if err_text.contains("Failed to parse route") || err_text.contains("Route did not match") {
        return (
            StatusCode::NOT_FOUND,
            "Page not found".to_string(),
            "The page you requested does not exist or may have moved.".to_string(),
            None,
        );
    }

    if let Some(error) = error.downcast_ref::<HttpError>() {
        return (
            error.status,
            match error.status {
                StatusCode::UNAUTHORIZED => "Please sign in".to_string(),
                StatusCode::NOT_FOUND => "Page not found".to_string(),
                _ => "Something went wrong".to_string(),
            },
            match error.status {
                StatusCode::UNAUTHORIZED => {
                    "Your session is missing or expired. Redirecting you to the sign-in page."
                        .to_string()
                }
                StatusCode::NOT_FOUND => {
                    "The page you requested does not exist or may have moved.".to_string()
                }
                _ => "The server could not complete this request.".to_string(),
            },
            error.message.clone(),
        );
    }

    if let Some(ServerFnError::ServerError { code, message, .. }) = error.downcast_ref() {
        let status = StatusCode::from_u16(*code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        return (
            status,
            match status {
                StatusCode::UNAUTHORIZED => "Please sign in".to_string(),
                StatusCode::NOT_FOUND => "Record not found".to_string(),
                _ => "Something went wrong".to_string(),
            },
            match status {
                StatusCode::UNAUTHORIZED => {
                    "Your session is missing or expired. Redirecting you to the sign-in page."
                        .to_string()
                }
                StatusCode::NOT_FOUND => "The data you asked for could not be found.".to_string(),
                _ => "The server hit an unexpected error while rendering this page.".to_string(),
            },
            Some(message.clone()),
        );
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "Something went wrong".to_string(),
        "The server hit an unexpected error while rendering this page.".to_string(),
        Some(error.to_string()),
    )
}

#[component]
pub fn LoginRedirectNotice(title: String, message: String) -> Element {
    let nav = use_navigator();

    use_effect(move || {
        nav.replace(crate::Route::Login {});
    });

    commit_login_redirect();

    rsx! {
        div { class: "status-shell",
            div { class: "status-panel",
                span { class: "eyebrow", "Authentication Required" }
                div { class: "status-code", "302" }
                h1 { class: "section-title", "{title}" }
                p { class: "supporting-text", "{message}" }
                div { class: "status-actions",
                    Link { to: crate::Route::Login {}, class: "btn btn-primary", "Go to login" }
                }
            }
        }
    }
}

#[component]
pub fn StatusPage(
    status_code: u16,
    eyebrow: String,
    title: String,
    message: String,
    primary_route: crate::Route,
    primary_label: String,
    detail: Option<String>,
) -> Element {
    rsx! {
        div { class: "status-shell",
            div { class: "status-panel",
                span { class: "eyebrow", "{eyebrow}" }
                div { class: "status-code", "{status_code}" }
                h1 { class: "section-title", "{title}" }
                p { class: "supporting-text", "{message}" }
                if let Some(detail) = detail.filter(|value| !value.trim().is_empty()) {
                    pre { class: "status-detail", "{detail}" }
                }
                div { class: "status-actions",
                    Link { to: primary_route, class: "btn btn-primary", "{primary_label}" }
                    Link {
                        to: crate::Route::Login {},
                        class: "btn btn-secondary",
                        "Sign in"
                    }
                }
            }
        }
    }
}

#[component]
pub fn InternalError(error: ErrorContext) -> Element {
    let _ = error;

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "message message-error",
                    div {
                        div { style: "font-size: 2rem; font-weight: 800; margin-bottom: 4px;",
                            "500"
                        }
                        div { style: "font-weight: 600; margin-bottom: 8px;", "Something went wrong" }
                        div { style: "opacity: 0.7; font-size: 0.875rem; margin-bottom: 16px;",
                            "An unexpected error occurred. Please try again or return to the dashboard."
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

#[component]
pub fn AppErrorPage(errors: ErrorContext) -> Element {
    let Some(error) = errors.error() else {
        return rsx! {
            div {}
        };
    };

    let (status, title, message, detail) = describe_error(&error);

    if status == StatusCode::UNAUTHORIZED {
        return rsx! {
            LoginRedirectNotice { title: title.clone(), message: message.clone() }
        };
    }

    commit_status(status, message.clone());

    if status == StatusCode::NOT_FOUND {
        return rsx! {
            crate::views::not_found::NotFound { segments: Vec::new() }
        };
    }

    if status == StatusCode::INTERNAL_SERVER_ERROR {
        return rsx! {
            InternalError { error: errors }
        };
    }

    let eyebrow = "Request Error".to_string();

    rsx! {
        StatusPage {
            status_code: status.as_u16(),
            eyebrow,
            title,
            message,
            primary_route: crate::Route::Dashboard {},
            primary_label: "Back to dashboard".to_string(),
            detail,
        }
    }
}
