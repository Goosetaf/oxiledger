//! The app's top-level destinations and how each user has arranged them.
//!
//! To add one, add a variant to [`Destination`] and the compiler asks for the rest.

use crate::Route;
use dioxus::prelude::*;
use dioxus_free_icons::{
    icons::ld_icons::{LdArrowLeftRight, LdLandmark, LdLayers, LdLayoutDashboard},
    Icon,
};
use serde::{Deserialize, Deserializer, Serialize};
use strum::VariantArray;

/// How many destinations fit beside "+" and "Menu" in the phone tab bar.
pub const SHORTCUT_SLOTS: usize = 3;

/// A top-level place in the app, listed in the order a new user sees them.
///
/// The names are saved in user preferences, so a renamed variant needs a `#[serde(alias)]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, VariantArray)]
#[serde(rename_all = "snake_case")]
pub enum Destination {
    Dashboard,
    Transactions,
    Accounts,
    BankAccounts,
}

/// What the navigation shows for a [`Destination`].
pub struct DestinationInfo {
    pub label: &'static str,
    /// Short enough to sit under an icon in the phone tab bar.
    pub short_label: &'static str,
    pub icon: fn() -> Element,
    pub route: Route,
}

impl Destination {
    /// Every destination, in the order a new user sees them.
    pub const ALL: &'static [Destination] = Self::VARIANTS;

    pub fn info(self) -> DestinationInfo {
        match self {
            Destination::Dashboard => DestinationInfo {
                label: "Dashboard",
                short_label: "Dashboard",
                icon: || rsx! {
                    Icon { class: "icon", icon: LdLayoutDashboard }
                },
                route: Route::Dashboard {},
            },
            Destination::Transactions => DestinationInfo {
                label: "Transactions",
                short_label: "Transactions",
                icon: || rsx! {
                    Icon { class: "icon", icon: LdArrowLeftRight }
                },
                route: Route::Transactions {},
            },
            Destination::Accounts => DestinationInfo {
                label: "Accounts",
                short_label: "Accounts",
                icon: || rsx! {
                    Icon { class: "icon", icon: LdLayers }
                },
                route: Route::Accounts {},
            },
            Destination::BankAccounts => DestinationInfo {
                label: "Bank Accounts",
                short_label: "Banks",
                icon: || rsx! {
                    Icon { class: "icon", icon: LdLandmark }
                },
                route: Route::BankAccounts {},
            },
        }
    }
}

impl Route {
    /// The destination highlighted in the navigation while on this page, if any.
    pub fn destination(&self) -> Option<Destination> {
        match self {
            Route::Dashboard {} => Some(Destination::Dashboard),
            Route::Transactions {}
            | Route::NewTransaction {}
            | Route::NewTransactionFromBank { .. }
            | Route::EditTransaction { .. } => Some(Destination::Transactions),
            Route::Accounts {} | Route::NewAccount {} | Route::EditAccount { .. } => {
                Some(Destination::Accounts)
            }
            Route::BankAccounts {}
            | Route::NewBankAccount {}
            | Route::BankAccountDetail { .. }
            | Route::BankAccountEdit { .. }
            | Route::BankConnect { .. }
            | Route::BankAccountMap { .. } => Some(Destination::BankAccounts),
            Route::Login {}
            | Route::Register {}
            | Route::OidcLogin {}
            | Route::OidcCallback {}
            | Route::Customize {}
            | Route::NotFound { .. } => None,
        }
    }
}

/// One user's arrangement of the navigation, with the menu order and phone shortcuts chosen separately.
///
/// Every destination is either shown or hidden, and shortcuts are always shown destinations.
/// Values read from storage or requests are repaired on the way in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "StoredNavPreferences")]
pub struct NavPreferences {
    shown: Vec<Destination>,
    hidden: Vec<Destination>,
    shortcuts: Vec<Destination>,
}

impl Default for NavPreferences {
    fn default() -> Self {
        Self {
            shown: Destination::ALL.to_vec(),
            hidden: Vec::new(),
            shortcuts: Destination::ALL.iter().copied().take(SHORTCUT_SLOTS).collect(),
        }
    }
}

impl NavPreferences {
    /// Destinations in the sidebar and the phone menu, in that order.
    pub fn shown(&self) -> &[Destination] {
        &self.shown
    }

    /// Destinations in the phone tab bar, left to right. At most
    /// [`SHORTCUT_SLOTS`], with no gaps.
    pub fn shortcuts(&self) -> &[Destination] {
        &self.shortcuts
    }

    /// Destinations left out of the navigation. Their pages still open by link.
    pub fn hidden(&self) -> &[Destination] {
        &self.hidden
    }

    /// Swaps a shown destination with the one before it.
    pub fn move_earlier(&mut self, destination: Destination) {
        if let Some(index) = self.shown.iter().position(|d| *d == destination) {
            if index > 0 {
                self.shown.swap(index, index - 1);
            }
        }
    }

    /// Swaps a shown destination with the one after it.
    pub fn move_later(&mut self, destination: Destination) {
        if let Some(index) = self.shown.iter().position(|d| *d == destination) {
            if index + 1 < self.shown.len() {
                self.shown.swap(index, index + 1);
            }
        }
    }

    /// Takes a destination out of the navigation, and out of the tab bar if it
    /// was a shortcut.
    pub fn hide(&mut self, destination: Destination) {
        if let Some(index) = self.shown.iter().position(|d| *d == destination) {
            self.shown.remove(index);
            self.hidden.push(destination);
            self.shortcuts.retain(|d| *d != destination);
        }
    }

    /// Brings a hidden destination back, after the ones already shown.
    pub fn show(&mut self, destination: Destination) {
        if let Some(index) = self.hidden.iter().position(|d| *d == destination) {
            self.hidden.remove(index);
            self.shown.push(destination);
        }
    }

    /// Puts a shown destination in a tab bar slot, or empties the slot with `None`.
    ///
    /// Picking one that already has a slot swaps the two. Emptying a slot closes the gap,
    /// and a slot past the last shortcut means the first free one.
    pub fn set_shortcut(&mut self, slot: usize, destination: Option<Destination>) {
        if slot >= SHORTCUT_SLOTS {
            return;
        }

        let Some(destination) = destination else {
            if slot < self.shortcuts.len() {
                self.shortcuts.remove(slot);
            }
            return;
        };
        if !self.shown.contains(&destination) {
            return;
        }

        match self.shortcuts.iter().position(|d| *d == destination) {
            Some(current) => {
                let target = slot.min(self.shortcuts.len() - 1);
                self.shortcuts.swap(current, target);
            }
            None if slot < self.shortcuts.len() => self.shortcuts[slot] = destination,
            None => self.shortcuts.push(destination),
        }
    }
}

/// [`NavPreferences`] as stored or submitted, before repair.
///
/// Unknown names and repeats are dropped, unmentioned destinations are shown last, and
/// shortcuts that are no longer shown are dropped. Only a value without shortcuts gets the defaults.
#[derive(Default, Deserialize)]
#[serde(default)]
struct StoredNavPreferences {
    #[serde(deserialize_with = "known_destinations")]
    shown: Vec<Destination>,
    #[serde(deserialize_with = "known_destinations")]
    hidden: Vec<Destination>,
    #[serde(deserialize_with = "known_destinations_if_present")]
    shortcuts: Option<Vec<Destination>>,
}

impl From<StoredNavPreferences> for NavPreferences {
    fn from(stored: StoredNavPreferences) -> Self {
        let mut hidden = Vec::new();
        for destination in stored.hidden {
            if !hidden.contains(&destination) {
                hidden.push(destination);
            }
        }

        let mut shown = Vec::new();
        for destination in stored.shown.into_iter().chain(Destination::ALL.iter().copied()) {
            if !shown.contains(&destination) && !hidden.contains(&destination) {
                shown.push(destination);
            }
        }

        let chosen = stored.shortcuts.unwrap_or_else(|| shown.clone());
        let mut shortcuts = Vec::new();
        for destination in chosen {
            if shortcuts.len() < SHORTCUT_SLOTS
                && shown.contains(&destination)
                && !shortcuts.contains(&destination)
            {
                shortcuts.push(destination);
            }
        }

        Self {
            shown,
            hidden,
            shortcuts,
        }
    }
}

/// Reads a list of destination names, skipping the ones this build does not know.
fn known_destinations<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Destination>, D::Error> {
    use serde::de::value::{Error, StrDeserializer};

    let names = Vec::<String>::deserialize(deserializer)?;
    Ok(names
        .iter()
        .filter_map(|name| Destination::deserialize(StrDeserializer::<Error>::new(name)).ok())
        .collect())
}

/// As [`known_destinations`], telling a list that was never saved apart from
/// one saved empty.
fn known_destinations_if_present<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<Destination>>, D::Error> {
    known_destinations(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use super::Destination::{Accounts, BankAccounts, Dashboard, Transactions};
    use super::*;
    use serde_json::json;

    fn read(stored: serde_json::Value) -> NavPreferences {
        serde_json::from_value(stored).expect("navigation preferences should always be readable")
    }

    /// Every destination but the given ones, in the default order. Keeps these
    /// tests true when a destination is added.
    fn all_except(excluded: &[Destination]) -> Vec<Destination> {
        Destination::ALL
            .iter()
            .copied()
            .filter(|destination| !excluded.contains(destination))
            .collect()
    }

    #[test]
    fn a_new_user_sees_every_destination_in_the_default_order() {
        let preferences = NavPreferences::default();

        assert_eq!(preferences.shown(), Destination::ALL);
        assert!(preferences.hidden().is_empty());
        assert_eq!(read(json!({})), preferences);
    }

    #[test]
    fn stored_names_never_change() {
        // Saved preferences hold these names, so add new ones and only change one with an alias
        for (destination, name) in [
            (Dashboard, "dashboard"),
            (Transactions, "transactions"),
            (Accounts, "accounts"),
            (BankAccounts, "bank_accounts"),
        ] {
            assert_eq!(serde_json::to_value(destination).unwrap(), json!(name));
            assert_eq!(read(json!({ "shown": [name] })).shown()[0], destination);
        }
    }

    #[test]
    fn a_new_user_gets_the_first_destinations_as_phone_shortcuts() {
        let preferences = NavPreferences::default();

        assert_eq!(preferences.shortcuts(), &Destination::ALL[..SHORTCUT_SLOTS]);
    }

    #[test]
    fn phone_shortcuts_are_chosen_apart_from_the_order() {
        let mut preferences = NavPreferences::default();
        let shortcuts = preferences.shortcuts().to_vec();

        // Reordering the sidebar leaves the tab bar alone.
        preferences.move_later(Dashboard);
        preferences.move_earlier(BankAccounts);
        assert_eq!(preferences.shortcuts(), shortcuts);

        // And the tab bar can hold something from further down the list.
        preferences.set_shortcut(0, Some(BankAccounts));
        assert_eq!(preferences.shortcuts()[0], BankAccounts);
        assert_eq!(preferences.shortcuts().len(), SHORTCUT_SLOTS);
    }

    #[test]
    fn a_destination_takes_one_slot_at_most() {
        let mut preferences = NavPreferences::default();
        let [first, second, third] = [Dashboard, Transactions, Accounts];
        assert_eq!(preferences.shortcuts(), [first, second, third]);

        // Choosing one that already has a slot swaps the two.
        preferences.set_shortcut(0, Some(third));
        assert_eq!(preferences.shortcuts(), [third, second, first]);

        // Emptying a slot closes the gap.
        preferences.set_shortcut(0, None);
        assert_eq!(preferences.shortcuts(), [second, first]);

        // A slot past the end means the first free one.
        preferences.set_shortcut(2, Some(third));
        assert_eq!(preferences.shortcuts(), [second, first, third]);

        // There are only so many slots, and hidden destinations get none.
        preferences.set_shortcut(SHORTCUT_SLOTS, Some(BankAccounts));
        preferences.hide(BankAccounts);
        preferences.set_shortcut(0, Some(BankAccounts));
        assert_eq!(preferences.shortcuts(), [second, first, third]);
    }

    #[test]
    fn hiding_a_shortcut_frees_its_slot() {
        let mut preferences = NavPreferences::default();

        preferences.hide(Transactions);
        assert_eq!(preferences.shortcuts(), [Dashboard, Accounts]);

        // Showing it again does not put it back in the tab bar.
        preferences.show(Transactions);
        assert_eq!(preferences.shortcuts(), [Dashboard, Accounts]);
    }

    #[test]
    fn moving_swaps_neighbours_and_stops_at_the_ends() {
        let (first, second) = (Destination::ALL[0], Destination::ALL[1]);
        let last = Destination::ALL[Destination::ALL.len() - 1];
        let mut preferences = NavPreferences::default();

        preferences.move_later(first);
        assert_eq!(preferences.shown()[..2], [second, first]);

        preferences.move_earlier(first);
        assert_eq!(preferences.shown(), Destination::ALL);

        // Nothing moves past the ends
        preferences.move_earlier(first);
        preferences.move_later(last);
        assert_eq!(preferences.shown(), Destination::ALL);
    }

    #[test]
    fn hiding_and_showing_keeps_every_destination_accounted_for() {
        let mut preferences = NavPreferences::default();

        preferences.hide(Transactions);
        assert_eq!(preferences.shown(), all_except(&[Transactions]));
        assert_eq!(preferences.hidden(), [Transactions]);

        // Hidden destinations cannot be moved or hidden again.
        preferences.move_earlier(Transactions);
        preferences.hide(Transactions);
        assert_eq!(preferences.shown(), all_except(&[Transactions]));
        assert_eq!(preferences.hidden(), [Transactions]);

        // Showing one again puts it after everything already shown.
        preferences.show(Transactions);
        assert_eq!(preferences.shown().last(), Some(&Transactions));
        assert_eq!(preferences.shown().len(), Destination::ALL.len());
        assert!(preferences.hidden().is_empty());
    }

    #[test]
    fn what_is_saved_is_read_back_unchanged() {
        let mut preferences = NavPreferences::default();
        preferences.move_later(Dashboard);
        preferences.hide(Accounts);

        preferences.set_shortcut(0, Some(BankAccounts));

        let saved = serde_json::to_value(&preferences).unwrap();

        assert_eq!(saved["shown"][1], json!("dashboard"));
        assert_eq!(saved["hidden"], json!(["accounts"]));
        assert_eq!(saved["shortcuts"][0], json!("bank_accounts"));
        assert_eq!(read(saved), preferences);
    }

    #[test]
    fn a_destination_added_after_saving_appears_last() {
        // Saved when only these two existed.
        let preferences = read(json!({ "shown": ["transactions", "dashboard"] }));

        let mut expected = vec![Transactions, Dashboard];
        expected.extend(all_except(&[Transactions, Dashboard]));
        assert_eq!(preferences.shown(), expected);
        assert!(preferences.hidden().is_empty());
    }

    #[test]
    fn a_destination_added_after_saving_takes_no_shortcut_slot() {
        // Saved with one slot deliberately left free.
        let preferences = read(json!({
            "shown": ["transactions", "dashboard"],
            "shortcuts": ["dashboard", "transactions"],
        }));

        assert_eq!(preferences.shortcuts(), [Dashboard, Transactions]);
        assert_eq!(read(json!({ "shortcuts": [] })).shortcuts(), []);
    }

    #[test]
    fn preferences_saved_before_shortcuts_existed_get_the_first_shown() {
        let preferences = read(json!({ "shown": ["bank_accounts", "accounts"] }));

        assert_eq!(preferences.shortcuts(), &preferences.shown()[..SHORTCUT_SLOTS]);
        assert_eq!(preferences.shortcuts()[0], BankAccounts);
    }

    #[test]
    fn stale_or_damaged_preferences_are_repaired() {
        let preferences = read(json!({
            "shown": ["accounts", "retired_page", "accounts", "dashboard", "transactions"],
            "hidden": ["transactions", "transactions", "another_retired_page"],
        }));

        // Unknown names and repeats are dropped, and hidden wins over shown.
        let mut expected = vec![Accounts, Dashboard];
        expected.extend(all_except(&[Accounts, Dashboard, Transactions]));
        assert_eq!(preferences.shown(), expected);
        assert_eq!(preferences.hidden(), [Transactions]);

        // Shortcuts get the same repair, and cannot point at something hidden.
        let preferences = read(json!({
            "hidden": ["transactions"],
            "shortcuts": ["transactions", "accounts", "retired_page", "accounts", "dashboard"],
        }));
        assert_eq!(preferences.shortcuts(), [Accounts, Dashboard]);
    }

    #[test]
    fn each_destination_owns_the_page_it_links_to() {
        for &destination in Destination::ALL {
            assert_eq!(destination.info().route.destination(), Some(destination));
        }
    }
}
