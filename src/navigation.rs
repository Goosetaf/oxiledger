//! The app's top-level destinations and how each user has arranged them.
//!
//! The sidebar, the phone tab bar, the phone menu and the Customize page all
//! render from this module, so a destination added here shows up in every one
//! of them. To add one:
//!
//! 1. add a variant to [`Destination`],
//! 2. describe it in [`Destination::info`],
//! 3. assign its pages in [`Route::destination`].
//!
//! The compiler asks for steps 2 and 3. Existing users get the new destination
//! at the end of their navigation; nothing they arranged moves.

use crate::Route;
use serde::{Deserialize, Deserializer, Serialize};
use strum::VariantArray;

/// How many destinations fit beside "+" and "Menu" in the phone tab bar.
pub const SHORTCUT_SLOTS: usize = 3;

/// A top-level place in the app. The variants are in the order a new user sees
/// them.
///
/// The serialized names are stored in user preferences: when renaming a
/// variant, keep the old name readable with `#[serde(alias = "...")]`.
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
    /// SVG path data on a 24x24 grid, drawn as an outline.
    pub icon: &'static str,
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
                icon: "M4 5h7v7H4zM13 5h7v4h-7zM13 12h7v7h-7zM4 15h7v4H4z",
                route: Route::Dashboard {},
            },
            Destination::Transactions => DestinationInfo {
                label: "Transactions",
                short_label: "Transactions",
                icon: "M6 8h12M15 5l3 3-3 3M18 16H6M9 13l-3 3 3 3",
                route: Route::Transactions {},
            },
            Destination::Accounts => DestinationInfo {
                label: "Accounts",
                short_label: "Accounts",
                icon: "M12 4l8 4-8 4-8-4zM4 12l8 4 8-4M4 16l8 4 8-4",
                route: Route::Accounts {},
            },
            Destination::BankAccounts => DestinationInfo {
                label: "Bank Accounts",
                short_label: "Banks",
                icon: "M3 10l9-6 9 6M5 10v8M9.7 10v8M14.3 10v8M19 10v8M3 20h18",
                route: Route::BankAccounts {},
            },
        }
    }
}

impl Route {
    /// The destination a page belongs to, which is the one the navigation
    /// highlights while you are on it. Pages reached from outside the
    /// destination list (signing in, Customize) belong to none.
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

/// One user's arrangement of the navigation.
///
/// Every destination is either shown or hidden, never both and never neither.
/// That holds for any value of this type, however it was produced: reading it
/// from the database or from a request repairs what is stale (see
/// [`StoredNavPreferences`]), and the editing methods keep it true.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "StoredNavPreferences")]
pub struct NavPreferences {
    shown: Vec<Destination>,
    hidden: Vec<Destination>,
}

impl Default for NavPreferences {
    fn default() -> Self {
        Self {
            shown: Destination::ALL.to_vec(),
            hidden: Vec::new(),
        }
    }
}

impl NavPreferences {
    /// Destinations in the sidebar and the phone menu, most used first.
    pub fn shown(&self) -> &[Destination] {
        &self.shown
    }

    /// The first few shown destinations: the ones in the phone tab bar.
    pub fn shortcuts(&self) -> &[Destination] {
        &self.shown[..self.shown.len().min(SHORTCUT_SLOTS)]
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

    pub fn hide(&mut self, destination: Destination) {
        if let Some(index) = self.shown.iter().position(|d| *d == destination) {
            self.shown.remove(index);
            self.hidden.push(destination);
        }
    }

    /// Brings a hidden destination back, after the ones already shown.
    pub fn show(&mut self, destination: Destination) {
        if let Some(index) = self.hidden.iter().position(|d| *d == destination) {
            self.hidden.remove(index);
            self.shown.push(destination);
        }
    }
}

/// [`NavPreferences`] as it arrives from storage or a request, before repair.
///
/// It may predate a destination, name one that no longer exists, or list one
/// twice. Converting it keeps what the user chose and fills in the rest:
/// unknown names are dropped, and destinations it never mentions are shown
/// after the ones it does.
#[derive(Default, Deserialize)]
#[serde(default)]
struct StoredNavPreferences {
    #[serde(deserialize_with = "known_destinations")]
    shown: Vec<Destination>,
    #[serde(deserialize_with = "known_destinations")]
    hidden: Vec<Destination>,
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

        Self { shown, hidden }
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
        // Saved preferences hold these names. Add a line for a new destination;
        // an existing line only changes together with a `#[serde(alias)]`.
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
    fn the_first_shown_destinations_are_the_phone_shortcuts() {
        let mut preferences = NavPreferences::default();
        assert_eq!(preferences.shortcuts(), &Destination::ALL[..SHORTCUT_SLOTS]);

        // Hiding a shortcut hands its slot to the next destination in line.
        preferences.hide(Dashboard);
        assert_eq!(preferences.shortcuts(), &Destination::ALL[1..=SHORTCUT_SLOTS]);

        // With fewer destinations than slots, all of them are shortcuts.
        let mut few = NavPreferences::default();
        for destination in all_except(&[Transactions, Accounts]) {
            few.hide(destination);
        }
        assert_eq!(few.shortcuts(), [Transactions, Accounts]);
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

        // Already at the ends: nothing moves.
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

        let saved = serde_json::to_value(&preferences).unwrap();

        assert_eq!(saved["shown"][1], json!("dashboard"));
        assert_eq!(saved["hidden"], json!(["accounts"]));
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
    }

    #[test]
    fn each_destination_owns_the_page_it_links_to() {
        for &destination in Destination::ALL {
            assert_eq!(destination.info().route.destination(), Some(destination));
        }
    }
}
