use dioxus::prelude::*;

/// Reusable button component to centralize styles and accessibility.
#[component]
pub fn Button(
    class: Option<String>,
    r#type: Option<String>,
    disabled: Option<bool>,
    aria_label: Option<String>,
    onclick: Option<Callback<Event<MouseData>>>,
    children: Element,
) -> Element {
    let classes = class.unwrap_or_else(|| "btn btn-primary".to_string());

    let cb = onclick.clone();

    rsx! {
        button {
            class: "{classes}",
            r#type: r#type.as_deref().unwrap_or("button"),
            disabled: disabled.unwrap_or(false),
            aria_label: aria_label.as_deref(),
            onclick: move |e| {
                if let Some(cb) = cb.clone() {
                    cb.call(e);
                }
            },
            {children}
        }
    }
}
