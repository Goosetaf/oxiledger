//! Everything a user has tailored, stored as one JSON object in `users.preferences`.
//!
//! Each feature owns one field of [`Preferences`], so a new preference is just a new field.

use crate::navigation::NavPreferences;
use dioxus::prelude::*;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default, deserialize_with = "or_default")]
    pub navigation: NavPreferences,
}

/// Reads one feature's preferences, falling back to its defaults when they
/// cannot be read, so one bad field never resets the others.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let stored = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(stored).unwrap_or_default())
}

#[post("/api/preferences")]
async fn save_preferences(preferences: Preferences) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let stored =
            serde_json::to_value(&preferences).map_err(|e| ServerFnError::new(e.to_string()))?;

        sqlx::query!(
            "UPDATE users SET preferences = $1 WHERE id = $2",
            stored,
            user_id
        )
        .execute(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;
    }

    Ok(())
}

#[derive(Clone, Copy)]
pub struct PreferencesStore {
    current: Signal<Preferences>,
    save_error: Signal<Option<String>>,
    set: Callback<Preferences>,
}

impl PreferencesStore {
    pub fn get(&self) -> Preferences {
        (self.current)()
    }

    /// Why the latest change could not be saved, after it has been undone.
    pub fn save_error(&self) -> Option<String> {
        (self.save_error)()
    }

    /// Shows `next` right away and saves it in the background.
    ///
    /// Saves run in order, changes made during a save are sent together after it, and a
    /// failed save puts back what was last saved.
    pub fn set(&self, next: Preferences) {
        self.set.call(next);
    }
}

pub fn use_preferences() -> PreferencesStore {
    use_context()
}

/// Owns the preferences for as long as the calling component is mounted, so a
/// save started on one page finishes even after leaving it.
pub fn use_preferences_provider(initial: Preferences) -> PreferencesStore {
    let mut current = use_signal(|| initial.clone());
    // Last saved value, which a failed save reverts to
    let mut saved = use_signal(|| initial);
    let mut save_error = use_signal(|| None::<String>);
    let mut saving = use_signal(|| false);
    let mut queued = use_signal(|| None::<Preferences>);

    let set = use_callback(move |next: Preferences| {
        current.set(next.clone());
        save_error.set(None);

        if saving() {
            // Picked up by the save in flight
            queued.set(Some(next));
            return;
        }

        saving.set(true);
        spawn(async move {
            let mut next = next;
            loop {
                match save_preferences(next.clone()).await {
                    Ok(()) => {
                        saved.set(next);
                        save_error.set(None);
                    }
                    Err(err) => save_error.set(Some(err.to_string())),
                }

                match queued.take() {
                    Some(newer) => next = newer,
                    None => break,
                }
            }

            if save_error.peek().is_some() {
                current.set(saved());
            }
            saving.set(false);
        });
    });

    use_context_provider(|| PreferencesStore {
        current,
        save_error,
        set,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_or_unreadable_preferences_fall_back_to_defaults() {
        let read = |stored| serde_json::from_value::<Preferences>(stored).unwrap();

        assert_eq!(read(json!({})), Preferences::default());
        assert_eq!(read(json!({ "navigation": 5 })), Preferences::default());
    }

    #[test]
    fn what_is_saved_is_read_back_unchanged() {
        let mut preferences = Preferences::default();
        preferences.navigation.move_later(crate::navigation::Destination::Dashboard);

        let saved = serde_json::to_value(&preferences).unwrap();

        assert_eq!(serde_json::from_value::<Preferences>(saved).unwrap(), preferences);
    }
}
