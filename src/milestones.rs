use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Something a user has done that the rest of their data can't show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Milestone {
    /// Recorded by the server in the same transaction that applies setup.
    SetupFinished,
    SetupSkipped,
    Dismissed(Hint),
}

/// A hint the user can close for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Hint {
    /// The checklist on the dashboard.
    GettingStarted,
}

#[cfg(any(feature = "server", test))]
impl Hint {
    pub const ALL: &[Hint] = &[Hint::GettingStarted];

    fn key(self) -> &'static str {
        match self {
            Hint::GettingStarted => "getting_started",
        }
    }
}

#[cfg(any(feature = "server", test))]
impl Milestone {
    /// The key stored in the database, which must never change once released.
    pub fn key(self) -> String {
        match self {
            Milestone::SetupFinished => "setup_finished".to_string(),
            Milestone::SetupSkipped => "setup_skipped".to_string(),
            Milestone::Dismissed(hint) => format!("dismissed:{}", hint.key()),
        }
    }

    /// Reads a stored key back, or `None` for one this version doesn't know.
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "setup_finished" => Some(Milestone::SetupFinished),
            "setup_skipped" => Some(Milestone::SetupSkipped),
            _ => {
                let hint = key.strip_prefix("dismissed:")?;
                Hint::ALL
                    .iter()
                    .copied()
                    .find(|known| known.key() == hint)
                    .map(Milestone::Dismissed)
            }
        }
    }

    /// Whether the browser may record this, rather than the server code that earns it.
    pub fn user_can_record(self) -> bool {
        match self {
            Milestone::SetupFinished => false,
            Milestone::SetupSkipped | Milestone::Dismissed(_) => true,
        }
    }
}

/// How far a user has got, from recorded milestones and from what their data already shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub recorded: BTreeSet<Milestone>,
    pub has_accounts: bool,
    pub has_transactions: bool,
    /// Has a bank account kept up to date by bank sync.
    pub bank_connected: bool,
    /// Whether this server has bank sync, so bank steps only show when they can be done.
    pub bank_sync_available: bool,
}

impl Progress {
    pub fn has(&self, milestone: Milestone) -> bool {
        self.recorded.contains(&milestone)
    }

    /// New users are sent to setup until they finish or skip it. Anyone with accounts is past it.
    pub fn needs_setup(&self) -> bool {
        !self.has_accounts
            && !self.has(Milestone::SetupFinished)
            && !self.has(Milestone::SetupSkipped)
    }
}

#[get("/api/milestones")]
pub async fn get_progress() -> Result<Progress, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        let row = sqlx::query!(
            r#"SELECT
                ARRAY(SELECT key FROM user_milestones WHERE user_id = $1) AS "recorded!",
                EXISTS(SELECT 1 FROM accounts WHERE user_id = $1) AS "has_accounts!",
                EXISTS(SELECT 1 FROM transactions WHERE user_id = $1) AS "has_transactions!",
                EXISTS(
                    SELECT 1 FROM bank_accounts
                    WHERE user_id = $1 AND bank_connection_id IS NOT NULL
                ) AS "bank_connected!""#,
            user_id
        )
        .fetch_one(&pool)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

        return Ok(Progress {
            // Keys this version doesn't know are left in the table and ignored
            recorded: row
                .recorded
                .iter()
                .filter_map(|key| Milestone::from_key(key))
                .collect(),
            has_accounts: row.has_accounts,
            has_transactions: row.has_transactions,
            bank_connected: row.bank_connected,
            bank_sync_available: !crate::bank_sync::configured_provider_ids().is_empty(),
        });
    }

    #[allow(unreachable_code)]
    Ok(Progress::default())
}

#[post("/api/milestones")]
pub async fn record_milestone(milestone: Milestone) -> Result<(), ServerFnError> {
    #[cfg(feature = "server")]
    {
        use crate::server::auth::{extract_context, require_auth};

        let (pool, cookies) = extract_context().await?;
        let user_id = require_auth(&pool, &cookies).await?;

        if !milestone.user_can_record() {
            return Err(HttpError::new(
                StatusCode::FORBIDDEN,
                "Only the server can record this milestone",
            )
            .into());
        }

        record(&pool, user_id, milestone)
            .await
            .map_err(|e| ServerFnError::new(e.to_string()))?;
    }

    Ok(())
}

/// Records a milestone once, so recording it again keeps the first time.
#[cfg(feature = "server")]
pub async fn record<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    user_id: uuid::Uuid,
    milestone: Milestone,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO user_milestones (user_id, key) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        user_id,
        milestone.key()
    )
    .execute(executor)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn every_milestone() -> Vec<Milestone> {
        // Fails to compile when a variant is added, as a reminder to list it here
        match Milestone::SetupFinished {
            Milestone::SetupFinished | Milestone::SetupSkipped | Milestone::Dismissed(_) => {}
        }

        let mut all = vec![Milestone::SetupFinished, Milestone::SetupSkipped];
        all.extend(Hint::ALL.iter().copied().map(Milestone::Dismissed));
        all
    }

    #[test]
    fn every_key_reads_back_and_fits_the_column() {
        let mut keys = HashSet::new();

        for milestone in every_milestone() {
            let key = milestone.key();
            assert!(key.len() <= 100, "{key} is too long for the schema");
            assert_eq!(Milestone::from_key(&key), Some(milestone));
            assert!(keys.insert(key), "{milestone:?} shares a key");
        }
    }

    #[test]
    fn unknown_keys_are_ignored() {
        assert_eq!(Milestone::from_key(""), None);
        assert_eq!(Milestone::from_key("tour_seen"), None);
        assert_eq!(Milestone::from_key("dismissed:"), None);
        assert_eq!(Milestone::from_key("dismissed:something_new"), None);
    }

    #[test]
    fn only_the_server_records_finishing_setup() {
        assert!(!Milestone::SetupFinished.user_can_record());
        assert!(Milestone::SetupSkipped.user_can_record());
        assert!(Milestone::Dismissed(Hint::GettingStarted).user_can_record());
    }

    #[test]
    fn setup_is_needed_until_finished_skipped_or_accounts_exist() {
        let mut progress = Progress::default();
        assert!(progress.needs_setup());

        progress
            .recorded
            .insert(Milestone::Dismissed(Hint::GettingStarted));
        assert!(progress.needs_setup());

        for milestone in [Milestone::SetupFinished, Milestone::SetupSkipped] {
            let mut done = progress.clone();
            done.recorded.insert(milestone);
            assert!(!done.needs_setup());
        }

        progress.has_accounts = true;
        assert!(!progress.needs_setup());
    }
}
