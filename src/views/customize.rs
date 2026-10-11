use crate::{
    components::{icon::Icon, nav::use_nav_settings},
    navigation::{Destination, NavPreferences},
};
use dioxus::prelude::*;

const ICON_UP: &str = "M6 15l6-6 6 6";
const ICON_DOWN: &str = "M6 9l6 6 6-6";
const ICON_SHOWN: &str =
    "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z";
const ICON_HIDDEN: &str = "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM4 4l16 16";

#[component]
pub fn Customize() -> Element {
    let settings = use_nav_settings();
    let preferences = settings.preferences();
    let shown = preferences.shown();
    let shortcuts = preferences.shortcuts();

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
                    h1 { class: "section-title", "Customize" }
                }

                if let Some(err) = settings.save_error() {
                    div { class: "message message-error", "Not saved, so the change was undone: {err}" }
                }

                section { class: "section-card",
                    div { class: "pref-header",
                        h2 { class: "section-title", "Navigation" }
                        button {
                            class: "btn btn-secondary btn-sm",
                            r#type: "button",
                            "aria-label": "Reset navigation",
                            disabled: preferences == NavPreferences::default(),
                            onclick: move |_| settings.update(NavPreferences::default()),
                            "Reset"
                        }
                    }

                    // The rows that make it into the phone tab bar are drawn as one
                    // block carrying the tab bar's own mark; the label names it once.
                    if !shortcuts.is_empty() {
                        p { class: "pref-caption", "Phone tab bar" }
                    }

                    ol { class: "pref-list",
                        for (index , destination) in shown.iter().copied().enumerate() {
                            li {
                                key: "{destination:?}",
                                class: if shortcuts.contains(&destination) { "pref-row is-shortcut" } else { "pref-row" },
                                Icon { path: destination.info().icon }
                                span { class: "pref-label", "{destination.info().label}" }
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
                                        class: "btn btn-secondary btn-sm btn-icon",
                                        r#type: "button",
                                        "aria-label": "Hide {destination.info().label}",
                                        onclick: move |_| edit(NavPreferences::hide, destination),
                                        Icon { path: ICON_SHOWN }
                                    }
                                }
                            }
                        }
                        for destination in preferences.hidden().iter().copied() {
                            li {
                                key: "{destination:?}",
                                class: "pref-row is-hidden",
                                Icon { path: destination.info().icon }
                                span { class: "pref-label", "{destination.info().label}" }
                                div { class: "pref-actions",
                                    button {
                                        class: "btn btn-secondary btn-sm btn-icon",
                                        r#type: "button",
                                        "aria-label": "Show {destination.info().label}",
                                        onclick: move |_| edit(NavPreferences::show, destination),
                                        Icon { path: ICON_HIDDEN }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
