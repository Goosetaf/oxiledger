use dioxus::prelude::*;

/// Reusable Card component (light glass + padding + optional title)
#[component]
pub fn Card(title: Option<String>, class: Option<String>, children: Element) -> Element {
    let classes = class
        .clone()
        .map(|c| format!("card {}", c))
        .unwrap_or_else(|| "card".to_string());

    rsx! {
        div { class: "{classes}",
            if let Some(title) = title.as_deref() {
                div { class: "section-header",
                    h2 { class: "card-title", "{title}" }
                }
            }
            {children}
        }
    }
}
