use std::{
    collections::HashMap,
    future::pending,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use tokio::sync::Mutex as AsyncMutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;

use crate::{
    bank_sync::provider::{AspspInfo, ProviderBalance},
    models::bank_sync::SyncResult,
};
use dioxus::prelude::ServerFnError;

const DEFAULT_ASPSP_CACHE_TTL_SECONDS: u64 = 60 * 60;
const DEFAULT_BACKGROUND_SCHEDULE: &str = "0 0 * * * *";

static ASPSP_CACHE: LazyLock<Mutex<HashMap<String, CachedAspspList>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static ACCOUNT_SYNC_LOCKS: LazyLock<Mutex<HashMap<Uuid, Arc<AsyncMutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
struct CachedAspspList {
    value: Vec<AspspInfo>,
    expires_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankSyncMode {
    Off,
    Lazy,
    Background,
}

#[derive(Clone, Debug)]
pub struct BankSyncSettings {
    pub mode: BankSyncMode,
    pub ttl_seconds: Option<i64>,
    pub schedule: Option<String>,
}

#[derive(Clone)]
struct SyncableBankAccount {
    id: Uuid,
    user_id: Uuid,
    provider_id: Option<String>,
    provider_session_id: Option<String>,
    provider_account_uid: Option<String>,
    last_synced_at: Option<DateTime<Utc>>,
    is_manual: bool,
}

impl BankSyncSettings {
    pub fn from_env() -> Self {
        let mode = match std::env::var("BANK_SYNC_MODE") {
            Ok(value) if value.eq_ignore_ascii_case("lazy") => BankSyncMode::Lazy,
            Ok(value) if value.eq_ignore_ascii_case("background") => BankSyncMode::Background,
            Ok(value) if value.eq_ignore_ascii_case("off") => BankSyncMode::Off,
            Ok(value) => {
                eprintln!("[bank-sync] Unknown BANK_SYNC_MODE '{value}', defaulting to 'off'");
                BankSyncMode::Off
            }
            Err(_) => BankSyncMode::Off,
        };

        let ttl_seconds = std::env::var("BANK_SYNC_TTL_SECONDS")
            .ok()
            .and_then(|value| match value.parse::<i64>() {
                Ok(parsed) if parsed > 0 => Some(parsed),
                Ok(_) => None,
                Err(err) => {
                    eprintln!(
                        "[bank-sync] Invalid BANK_SYNC_TTL_SECONDS '{value}': {err}. Ignoring TTL."
                    );
                    None
                }
            });

        let schedule = std::env::var("BANK_SYNC_SCHEDULE")
            .ok()
            .map(|value| normalize_cron_schedule(&value));

        Self {
            mode,
            ttl_seconds,
            schedule,
        }
    }

    pub fn should_lazy_sync(&self, last_synced_at: Option<DateTime<Utc>>) -> bool {
        self.mode == BankSyncMode::Lazy
            && self
                .ttl_seconds
                .is_some_and(|ttl_seconds| is_stale(last_synced_at, ttl_seconds))
    }

    pub fn should_background_sync(&self, last_synced_at: Option<DateTime<Utc>>) -> bool {
        if self.mode != BankSyncMode::Background {
            return false;
        }

        match self.ttl_seconds {
            Some(ttl_seconds) => is_stale(last_synced_at, ttl_seconds),
            None => true,
        }
    }

    pub fn background_schedule(&self) -> Option<String> {
        if self.mode != BankSyncMode::Background {
            return None;
        }

        Some(
            self.schedule
                .clone()
                .unwrap_or_else(|| DEFAULT_BACKGROUND_SCHEDULE.to_string()),
        )
    }
}

pub async fn list_aspsps_cached(
    provider_id: &str,
    country: Option<&str>,
) -> Result<Vec<AspspInfo>, ServerFnError> {
    let cache_key = format!("{provider_id}:{}", country.unwrap_or_default());

    {
        let cache = ASPSP_CACHE
            .lock()
            .map_err(|_| ServerFnError::new("ASPSP cache lock poisoned"))?;

        if let Some(entry) = cache.get(&cache_key) {
            if Instant::now() < entry.expires_at {
                return Ok(entry.value.clone());
            }
        }
    }

    let provider = crate::bank_sync::get_provider(provider_id).await?;
    let aspsps = provider
        .list_aspsps(country)
        .await
        .map_err(ServerFnError::from)?;

    let mut cache = ASPSP_CACHE
        .lock()
        .map_err(|_| ServerFnError::new("ASPSP cache lock poisoned"))?;
    cache.insert(
        cache_key,
        CachedAspspList {
            value: aspsps.clone(),
            expires_at: Instant::now() + Duration::from_secs(DEFAULT_ASPSP_CACHE_TTL_SECONDS),
        },
    );

    Ok(aspsps)
}

pub async fn maybe_lazy_sync_for_user(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<(), String> {
    let lock = account_sync_lock(id);
    let _guard = lock.lock().await;

    let settings = BankSyncSettings::from_env();
    if settings.mode != BankSyncMode::Lazy {
        return Ok(());
    }

    let Some(record) = load_syncable_bank_account(pool, id, Some(user_id)).await? else {
        return Err("Bank account not found".to_string());
    };

    if record.is_manual || !settings.should_lazy_sync(record.last_synced_at) {
        return Ok(());
    }

    sync_bank_account_record(pool, record, true).await?;
    Ok(())
}

pub async fn sync_bank_account_for_user(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    force: bool,
) -> Result<SyncResult, String> {
    let lock = account_sync_lock(id);
    let _guard = lock.lock().await;

    let record = load_syncable_bank_account(pool, id, Some(user_id))
        .await?
        .ok_or_else(|| "Bank account not found".to_string())?;

    if record.is_manual {
        return Err("Manual accounts do not support automatic sync.".to_string());
    }

    sync_bank_account_record(pool, record, force).await
}

pub async fn sync_bank_account_internal(
    pool: &PgPool,
    id: Uuid,
    force: bool,
) -> Result<SyncResult, String> {
    let lock = account_sync_lock(id);
    let _guard = lock.lock().await;

    let Some(record) = load_syncable_bank_account(pool, id, None).await? else {
        return Ok(SyncResult {
            new_count: 0,
            skipped_count: 0,
        });
    };

    if record.is_manual {
        return Ok(SyncResult {
            new_count: 0,
            skipped_count: 0,
        });
    }

    sync_bank_account_record(pool, record, force).await
}

pub async fn start_background_scheduler(pool: PgPool) {
    let settings = BankSyncSettings::from_env();
    let Some(schedule) = settings.background_schedule() else {
        return;
    };

    let job = match Job::new_async(schedule.clone(), move |_job_id, _scheduler| {
        let pool = pool.clone();
        Box::pin(async move {
            if let Err(err) = sync_stale_accounts(&pool).await {
                eprintln!("[bank-sync] Background sync failed: {err}");
            }
        })
    }) {
        Ok(job) => job,
        Err(err) => {
            eprintln!(
                "[bank-sync] Invalid BANK_SYNC_SCHEDULE '{schedule}': {err}. Background sync disabled."
            );
            return;
        }
    };

    let scheduler = match JobScheduler::new().await {
        Ok(scheduler) => scheduler,
        Err(err) => {
            eprintln!("[bank-sync] Failed to create background scheduler: {err}");
            return;
        }
    };

    if let Err(err) = scheduler.add(job).await {
        eprintln!("[bank-sync] Failed to register background sync job: {err}");
        return;
    }

    if let Err(err) = scheduler.start().await {
        eprintln!("[bank-sync] Failed to start background scheduler: {err}");
        return;
    }

    tokio::spawn(async move {
        let _scheduler = scheduler;
        pending::<()>().await;
    });
}

pub fn preferred_balance(balances: &[ProviderBalance]) -> Option<(Decimal, String)> {
    balances
        .iter()
        .find(|balance| balance.balance_type == "CLBD")
        .or_else(|| {
            balances
                .iter()
                .find(|balance| balance.balance_type == "CLAV")
        })
        .or_else(|| balances.first())
        .map(|balance| (balance.amount, balance.currency.clone()))
}

async fn sync_stale_accounts(pool: &PgPool) -> Result<(), String> {
    let settings = BankSyncSettings::from_env();
    if settings.mode != BankSyncMode::Background {
        return Ok(());
    }

    let rows = sqlx::query(
        r#"
        SELECT id, last_synced_at
        FROM bank_accounts
        WHERE is_manual = FALSE
          AND bank_connection_id IS NOT NULL
          AND provider_account_uid IS NOT NULL
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|err| err.to_string())?;

    for row in rows {
        let id: Uuid = row.get("id");
        let last_synced_at: Option<DateTime<Utc>> = row.get("last_synced_at");
        if !settings.should_background_sync(last_synced_at) {
            continue;
        }

        let force = settings.ttl_seconds.is_none();
        if let Err(err) = sync_bank_account_internal(pool, id, force).await {
            eprintln!("[bank-sync] Failed to sync account {id}: {err}");
        }
    }

    Ok(())
}

async fn sync_bank_account_record(
    pool: &PgPool,
    record: SyncableBankAccount,
    force: bool,
) -> Result<SyncResult, String> {
    let settings = BankSyncSettings::from_env();
    if !force {
        let Some(ttl_seconds) = settings.ttl_seconds else {
            return Ok(SyncResult {
                new_count: 0,
                skipped_count: 0,
            });
        };

        if !is_stale(record.last_synced_at, ttl_seconds) {
            return Ok(SyncResult {
                new_count: 0,
                skipped_count: 0,
            });
        }
    }

    let provider_id = record
        .provider_id
        .ok_or_else(|| "Bank account has no provider connection".to_string())?;
    let provider_session_id = record
        .provider_session_id
        .ok_or_else(|| "Bank account has no provider session".to_string())?;
    let provider_account_uid = record
        .provider_account_uid
        .ok_or_else(|| "Bank account has no provider account UID".to_string())?;

    let provider = crate::bank_sync::get_provider(&provider_id)
        .await
        .map_err(|err| err.to_string())?;

    let since = record.last_synced_at.map(|dt| dt.date_naive());
    let transactions = provider
        .fetch_transactions(&provider_session_id, &provider_account_uid, since)
        .await
        .map_err(|err| err.to_string())?;
    let balances = provider
        .get_balances(&provider_session_id, &provider_account_uid)
        .await
        .map_err(|err| err.to_string())?;
    let balance = preferred_balance(&balances).map(|(amount, _)| amount);

    let mut new_count = 0u32;
    let mut skipped_count = 0u32;

    for txn in &transactions {
        let result = sqlx::query(
            r#"
            INSERT INTO bank_transactions
                (bank_account_id, user_id, external_id, date, amount, currency,
                 description, reference)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (bank_account_id, external_id) DO NOTHING
            "#,
        )
        .bind(record.id)
        .bind(record.user_id)
        .bind(&txn.external_id)
        .bind(txn.date)
        .bind(txn.amount)
        .bind(&txn.currency)
        .bind(&txn.description)
        .bind(&txn.reference)
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;

        if result.rows_affected() > 0 {
            new_count += 1;
        } else {
            skipped_count += 1;
        }
    }

    sqlx::query(
        r#"
        UPDATE bank_accounts
        SET last_synced_at = NOW(),
            balance = $2
        WHERE id = $1
        "#,
    )
    .bind(record.id)
    .bind(balance)
    .execute(pool)
    .await
    .map_err(|err| err.to_string())?;

    Ok(SyncResult {
        new_count,
        skipped_count,
    })
}

async fn load_syncable_bank_account(
    pool: &PgPool,
    id: Uuid,
    user_id: Option<Uuid>,
) -> Result<Option<SyncableBankAccount>, String> {
    let row = match user_id {
        Some(user_id) => {
            sqlx::query(
                r#"
                SELECT
                    ba.id,
                    ba.user_id,
                    ba.provider_account_uid,
                    ba.last_synced_at,
                    ba.is_manual,
                    bc.provider_id,
                    bc.provider_session_id
                FROM bank_accounts ba
                LEFT JOIN bank_connections bc ON bc.id = ba.bank_connection_id
                WHERE ba.id = $1 AND ba.user_id = $2
                "#,
            )
            .bind(id)
            .bind(user_id)
            .fetch_optional(pool)
            .await
        }
        None => {
            sqlx::query(
                r#"
                SELECT
                    ba.id,
                    ba.user_id,
                    ba.provider_account_uid,
                    ba.last_synced_at,
                    ba.is_manual,
                    bc.provider_id,
                    bc.provider_session_id
                FROM bank_accounts ba
                LEFT JOIN bank_connections bc ON bc.id = ba.bank_connection_id
                WHERE ba.id = $1
                "#,
            )
            .bind(id)
            .fetch_optional(pool)
            .await
        }
    }
    .map_err(|err| err.to_string())?;

    Ok(row.map(|row| SyncableBankAccount {
        id: row.get("id"),
        user_id: row.get("user_id"),
        provider_id: row.get("provider_id"),
        provider_session_id: row.get("provider_session_id"),
        provider_account_uid: row.get("provider_account_uid"),
        last_synced_at: row.get("last_synced_at"),
        is_manual: row.get("is_manual"),
    }))
}

fn account_sync_lock(id: Uuid) -> Arc<AsyncMutex<()>> {
    let mut locks = ACCOUNT_SYNC_LOCKS
        .lock()
        .expect("bank sync lock map should not be poisoned");
    locks
        .entry(id)
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

fn is_stale(last_synced_at: Option<DateTime<Utc>>, ttl_seconds: i64) -> bool {
    match last_synced_at {
        Some(last_synced_at) => {
            Utc::now().signed_duration_since(last_synced_at)
                >= chrono::Duration::seconds(ttl_seconds)
        }
        None => true,
    }
}

fn normalize_cron_schedule(value: &str) -> String {
    let trimmed = value.trim();
    let part_count = trimmed.split_whitespace().count();
    if part_count == 5 {
        format!("0 {trimmed}")
    } else {
        trimmed.to_string()
    }
}
