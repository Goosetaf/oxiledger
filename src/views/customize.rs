use crate::{
    navigation::{Destination, NavPreferences, SHORTCUT_SLOTS},
    preferences::use_preferences,
};
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::ld_icons::{LdChevronDown, LdChevronUp, LdEye, LdEyeOff, LdPlus},
    Icon,
};

#[component]
pub fn Customize() -> Element {
    let store = use_preferences();
    let navigation = store.get().navigation;
    let shown = navigation.shown();

    rsx! {
        div { class: "app-container",
            div { class: "page-stack",
                div { class: "section-header",
                    h1 { class: "section-title", "Customize" }
                }

                if let Some(err) = store.save_error() {
                    div { class: "message message-error", "Not saved, so the change was undone: {err}" }
                }

                section { class: "section-card",
                    div { class: "pref-header",
                        h2 { class: "section-title", "Navigation" }
                        button {
                            class: "btn btn-secondary btn-sm",
                            r#type: "button",
                            "aria-label": "Reset navigation",
                            disabled: navigation == NavPreferences::default(),
                            onclick: move |_| {
                                let mut next = store.get();
                                next.navigation = NavPreferences::default();
                                store.set(next);
                            },
                            "Reset"
                        }
                    }

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

                    if !navigation.hidden().is_empty() {
                        div { class: "pref-group",
                            h3 { class: "field-label", "Hidden" }
                            ul { class: "pref-list",
                                for destination in navigation.hidden().iter().copied() {
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

#[component]
fn ShortcutSlot(slot: usize) -> Element {
    let store = use_preferences();
    let navigation = store.get().navigation;
    let chosen = navigation.shortcuts().get(slot).copied();
    // Slots fill from the left
    let reachable = slot <= navigation.shortcuts().len();

    rsx! {
        label { class: if chosen.is_some() { "slot" } else { "slot is-empty" },
            if let Some(destination) = chosen {
                {(destination.info().icon)()}
                span { class: "slot-label", "{destination.info().short_label}" }
            } else if reachable {
                Icon { class: "icon", icon: LdPlus }
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
                    let mut next = store.get();
                    next.navigation.set_shortcut(slot, choice);
                    store.set(next);
                },
                option { value: "", selected: chosen.is_none(), "Empty" }
                for (index , destination) in Destination::ALL.iter().copied().enumerate() {
                    // Free slots only offer what isn't in the bar yet
                    if navigation.shown().contains(&destination)
                        && (chosen.is_some() || !navigation.shortcuts().contains(&destination))
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

#[component]
fn DestinationRow(
    destination: Destination,
    #[props(default)] hidden: bool,
    #[props(default)] first: bool,
    #[props(default)] last: bool,
) -> Element {
    let store = use_preferences();
    let info = destination.info();

    let edit = move |change: fn(&mut NavPreferences, Destination)| {
        let mut next = store.get();
        change(&mut next.navigation, destination);
        store.set(next);
    };

    rsx! {
        li { class: if hidden { "pref-row is-hidden" } else { "pref-row" },
            {(info.icon)()}
            span { class: "pref-label", "{info.label}" }
            div { class: "pref-actions",
                if hidden {
                    button {
                        class: "btn btn-icon",
                        r#type: "button",
                        "aria-label": "Show {info.label}",
                        onclick: move |_| edit(NavPreferences::show),
                        Icon { class: "icon", icon: LdEyeOff }
                    }
                } else {
                    button {
                        class: "btn btn-icon",
                        r#type: "button",
                        "aria-label": "Move {info.label} up",
                        disabled: first,
                        onclick: move |_| edit(NavPreferences::move_earlier),
                        Icon { class: "icon", icon: LdChevronUp }
                    }
                    button {
                        class: "btn btn-icon",
                        r#type: "button",
                        "aria-label": "Move {info.label} down",
                        disabled: last,
                        onclick: move |_| edit(NavPreferences::move_later),
                        Icon { class: "icon", icon: LdChevronDown }
                    }
                    button {
                        class: "btn btn-icon",
                        r#type: "button",
                        "aria-label": "Hide {info.label}",
                        onclick: move |_| edit(NavPreferences::hide),
                        Icon { class: "icon", icon: LdEye }
                    }
                }
            }
        }
    }
}
