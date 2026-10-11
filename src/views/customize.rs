use crate::{
    components::{icon::Icon, nav::use_nav_settings},
    navigation::{Destination, NavPreferences, SHORTCUT_SLOTS},
};
use dioxus::prelude::*;

const ICON_UP: &str = "M6 15l6-6 6 6";
const ICON_DOWN: &str = "M6 9l6 6 6-6";

#[component]
pub fn Customize() -> Element {
    let settings = use_nav_settings();
    let preferences = settings.preferences();
    let shown = preferences.shown();
    let shortcuts = preferences.shortcuts();
    let hidden = preferences.hidden();

    // Applies one edit to a copy of the preferences and hands it to the shell,
    // which shows it at once and saves it.
    let edit = move |change: fn(&mut NavPreferences, Destination), destination: Destination| {
        let mut next = settings.preferences();
        change(&mut next, destination);
        settings.update(next);
    };

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    div {
                        h1 { class: "section-title", "Customize" }
                        p { class: "section-subtitle",
                            "Arrange OxiLedger around the parts of your economy you keep track of."
                        }
                    }
                }

                if let Some(err) = settings.save_error() {
                    div { class: "message message-error",
                        "That change could not be saved, so it was undone: {err}. Make it again to retry."
                    }
                }

                section { class: "section-card stack-lg",
                    div {
                        h2 { class: "section-title", "Navigation" }
                        p { class: "section-subtitle",
                            "Put what you use most first. This is the order of the sidebar and the phone menu, and the first {SHORTCUT_SLOTS} sit in the phone tab bar."
                        }
                    }

                    ol { class: "pref-list",
                        for (index , destination) in shown.iter().copied().enumerate() {
                            li { key: "{destination:?}", class: "pref-row",
                                Icon { path: destination.info().icon }
                                div { class: "pref-label",
                                    span { class: "label-strong", "{destination.info().label}" }
                                    if shortcuts.contains(&destination) {
                                        span { class: "tiny-text", "Phone shortcut" }
                                    }
                                }
                                div { class: "pref-actions",
                                    button {
                                        class: "btn btn-secondary btn-sm btn-icon",
                                        r#type: "button",
                                        "aria-label": "Move {destination.info().label} up",
                                        disabled: index == 0,
                                        onclick: move |_| edit(NavPreferences::move_earlier, destination),
                                        Icon { path: ICON_UP }
                                    }
                                    button {
                                        class: "btn btn-secondary btn-sm btn-icon",
                                        r#type: "button",
                                        "aria-label": "Move {destination.info().label} down",
                                        disabled: index + 1 == shown.len(),
                                        onclick: move |_| edit(NavPreferences::move_later, destination),
                                        Icon { path: ICON_DOWN }
                                    }
                                    button {
                                        class: "btn btn-secondary btn-sm",
                                        r#type: "button",
                                        "aria-label": "Hide {destination.info().label}",
                                        onclick: move |_| edit(NavPreferences::hide, destination),
                                        "Hide"
                                    }
                                }
                            }
                        }
                    }

                    if !hidden.is_empty() {
                        div {
                            h3 { class: "section-title", "Hidden" }
                            p { class: "section-subtitle",
                                "Left out of the sidebar and the phone menu. Their pages still open from links."
                            }
                        }

                        ul { class: "pref-list",
                            for destination in hidden.iter().copied() {
                                li {
                                    key: "{destination:?}",
                                    class: "pref-row is-hidden",
                                    Icon { path: destination.info().icon }
                                    div { class: "pref-label",
                                        span { "{destination.info().label}" }
                                    }
                                    div { class: "pref-actions",
                                        button {
                                            class: "btn btn-secondary btn-sm",
                                            r#type: "button",
                                            "aria-label": "Show {destination.info().label}",
                                            onclick: move |_| edit(NavPreferences::show, destination),
                                            "Show"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "actions-row",
                        button {
                            class: "btn btn-secondary",
                            r#type: "button",
                            disabled: preferences == NavPreferences::default(),
                            onclick: move |_| settings.update(NavPreferences::default()),
                            "Reset navigation"
                        }
                        span { class: "field-note", "Changes save as you make them." }
                    }
                }
            }
        }
    }
}
