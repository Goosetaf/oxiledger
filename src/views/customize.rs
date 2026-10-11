use crate::{
    components::{icon::Icon, nav::use_nav_settings},
    navigation::{Destination, NavPreferences, SHORTCUT_SLOTS},
};
use dioxus::prelude::*;

const ICON_UP: &str = "M6 15l6-6 6 6";
const ICON_DOWN: &str = "M6 9l6 6 6-6";
const ICON_SHOWN: &str =
    "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z";
const ICON_HIDDEN: &str = "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12zM4 4l16 16";
const ICON_ADD: &str = "M12 5v14M5 12h14";

#[component]
pub fn Customize() -> Element {
    let settings = use_nav_settings();
    let preferences = settings.preferences();
    let shown = preferences.shown();

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

                    // Drawn like the bar it edits: one slot per shortcut.
                    div { class: "pref-group",
                        h3 { class: "field-label", "Phone tab bar" }
                        div { class: "slot-bar",
                            for slot in 0..SHORTCUT_SLOTS {
                                ShortcutSlot { key: "{slot}", slot }
                            }
                        }
                    }

                    div { class: "pref-group",
                        h3 { class: "field-label", "Menu" }
                        ol { class: "pref-list",
                            for destination in shown.iter().copied() {
                                DestinationRow {
                                    key: "{destination:?}",
                                    destination,
                                    first: shown.first() == Some(&destination),
                                    last: shown.last() == Some(&destination),
                                }
                            }
                        }
                    }

                    if !preferences.hidden().is_empty() {
                        div { class: "pref-group",
                            h3 { class: "field-label", "Hidden" }
                            ul { class: "pref-list",
                                for destination in preferences.hidden().iter().copied() {
                                    DestinationRow {
                                        key: "{destination:?}",
                                        destination,
                                        hidden: true,
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

/// One slot of the phone tab bar. It shows its shortcut the way the tab bar
/// does, with a native picker laid over it to change or clear it.
#[component]
fn ShortcutSlot(slot: usize) -> Element {
    let settings = use_nav_settings();
    let preferences = settings.preferences();
    let chosen = preferences.shortcuts().get(slot).copied();
    // Slots fill from the left, so past the first free one there is nothing to pick yet.
    let reachable = slot <= preferences.shortcuts().len();

    rsx! {
        label { class: if chosen.is_some() { "slot" } else { "slot is-empty" },
            if let Some(destination) = chosen {
                Icon { path: destination.info().icon }
                span { class: "slot-label", "{destination.info().short_label}" }
            } else if reachable {
                Icon { path: ICON_ADD }
            }
            select {
                class: "slot-select",
                "aria-label": "Tab bar slot {slot + 1}",
                disabled: !reachable,
                onchange: move |event| {
                    let choice = event
                        .value()
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| Destination::ALL.get(index).copied());
                    let mut next = settings.preferences();
                    next.set_shortcut(slot, choice);
                    settings.update(next);
                },
                option { value: "", selected: chosen.is_none(), "Empty" }
                for (index , destination) in Destination::ALL.iter().copied().enumerate() {
                    // A filled slot offers every shown destination (picking one from
                    // another slot swaps them); a free slot only the ones not in the bar.
                    if preferences.shown().contains(&destination)
                        && (chosen.is_some() || !preferences.shortcuts().contains(&destination))
                    {
                        option {
                            value: "{index}",
                            selected: chosen == Some(destination),
                            "{destination.info().label}"
                        }
                    }
                }
            }
        }
    }
}

/// One destination with its controls. `first` and `last` are its position among
/// the shown destinations, which is where moving stops.
#[component]
fn DestinationRow(
    destination: Destination,
    #[props(default)] hidden: bool,
    #[props(default)] first: bool,
    #[props(default)] last: bool,
) -> Element {
    let settings = use_nav_settings();
    let info = destination.info();

    // Applies one edit to a copy of the preferences and hands it to the shell,
    // which shows it at once and saves it.
    let edit = move |change: fn(&mut NavPreferences, Destination)| {
        let mut next = settings.preferences();
        change(&mut next, destination);
        settings.update(next);
    };

    rsx! {
        li { class: if hidden { "pref-row is-hidden" } else { "pref-row" },
            Icon { path: info.icon }
            span { class: "pref-label", "{info.label}" }
            div { class: "pref-actions",
                if hidden {
                    button {
                        class: "btn btn-icon",
                        r#type: "button",
                        "aria-label": "Show {info.label}",
                        onclick: move |_| edit(NavPreferences::show),
                        Icon { path: ICON_HIDDEN }
                    }
                } else {
                    button {
                        class: "btn btn-icon",
                        r#type: "button",
                        "aria-label": "Move {info.label} up",
                        disabled: first,
                        onclick: move |_| edit(NavPreferences::move_earlier),
                        Icon { path: ICON_UP }
                    }
                    button {
                        class: "btn btn-icon",
                        r#type: "button",
                        "aria-label": "Move {info.label} down",
                        disabled: last,
                        onclick: move |_| edit(NavPreferences::move_later),
                        Icon { path: ICON_DOWN }
                    }
                    button {
                        class: "btn btn-icon",
                        r#type: "button",
                        "aria-label": "Hide {info.label}",
                        onclick: move |_| edit(NavPreferences::hide),
                        Icon { path: ICON_SHOWN }
                    }
                }
            }
        }
    }
}
