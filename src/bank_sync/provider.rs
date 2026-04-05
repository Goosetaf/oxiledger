use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::error::BankSyncError;

// ── Shared data types ──────────────────────────────────────────────────────────

/// A bank account as returned by the provider after authorization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBankAccount {
    /// Provider-side opaque UID (used in subsequent API calls).
    pub uid: String,
    /// Human-readable account name (may be absent for some banks).
    pub name: Option<String>,
    /// IBAN if available.
    pub iban: Option<String>,
    /// ISO 4217 currency code (e.g. "EUR").
    pub currency: String,
}

/// A balance entry for a bank account as returned by the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBalance {
    /// Amount (always positive; interpret with `balance_type`).
    pub amount: Decimal,
    /// ISO 4217 currency code.
    pub currency: String,
    /// Provider balance type string, e.g. "CLBD" (closing booked), "CLAV" (closing available).
    pub balance_type: String,
}

/// A single transaction as fetched from the provider.
/// `amount` is signed: positive = money coming in (inflow/credit),
/// negative = money going out (outflow/debit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTransaction {
    /// Stable external ID for idempotent import.
    pub external_id: String,
    /// Booking date.
    pub date: NaiveDate,
    /// Signed amount; positive = inflow.
    pub amount: Decimal,
    /// Currency code.
    pub currency: String,
    /// Human-readable description / remittance information.
    pub description: String,
    /// Optional reference number.
    pub reference: Option<String>,
}

// ── Provider trait ─────────────────────────────────────────────────────────────

/// Abstraction over a bank data provider.
///
/// Implement this trait to add a new provider (e.g. GoCardless BankAccountData).
/// The rest of the application only depends on this trait — all Enable Banking–
/// specific logic lives in [`super::enable_banking::EnableBankingProvider`].
///
/// All methods are `async`; use `async_trait` or Rust 1.75+ RPITIT as needed.
/// Currently implemented as plain `async fn` in the trait (stabilised in Rust 1.75).
#[allow(async_fn_in_trait)]
pub trait BankSyncProvider: Send + Sync {
    /// A stable, lowercase ASCII identifier for this provider, e.g. `"enable_banking"`.
    fn provider_id(&self) -> &str;

    /// A human-readable display name, e.g. `"Enable Banking"`.
    fn display_name(&self) -> &str;

    /// Fetch the list of ASPSPs (banks) available, optionally filtered by ISO country code.
    async fn list_aspsps(&self, country: Option<&str>) -> Result<Vec<AspspInfo>, BankSyncError>;

    /// Begin the bank authorization flow.
    ///
    /// Returns the URL the user should be redirected to.
    ///
    /// * `redirect_url` — where the provider should redirect back after consent.
    /// * `state`        — opaque CSRF token; the provider must echo it back.
    /// * `aspsp_name`   — bank name as returned by `list_aspsps`.
    /// * `aspsp_country`— ISO country code of the ASPSP.
    async fn start_authorization(
        &self,
        redirect_url: &str,
        state: &str,
        aspsp_name: &str,
        aspsp_country: &str,
    ) -> Result<String, BankSyncError>;

    /// Complete the authorization flow using the code returned by the provider.
    ///
    /// Returns the provider-side session ID (stored in `bank_connections`).
    /// Also returns the accounts accessible in this session.
    async fn complete_authorization(
        &self,
        code: &str,
    ) -> Result<(String, Vec<ProviderBankAccount>), BankSyncError>;

    /// Fetch balances for a specific bank account.
    ///
    /// * `session_id`   — provider session ID from `complete_authorization`.
    /// * `account_uid`  — provider account UID from `ProviderBankAccount::uid`.
    async fn get_balances(
        &self,
        session_id: &str,
        account_uid: &str,
    ) -> Result<Vec<ProviderBalance>, BankSyncError>;

    /// Fetch transactions for a bank account since an optional date.
    ///
    /// Returns only booked (settled) transactions.
    async fn fetch_transactions(
        &self,
        session_id: &str,
        account_uid: &str,
        since: Option<NaiveDate>,
    ) -> Result<Vec<ProviderTransaction>, BankSyncError>;

    /// Revoke / close the provider-side session.
    async fn revoke_session(&self, session_id: &str) -> Result<(), BankSyncError>;
}

/// Minimal information about an ASPSP (bank), safe to display in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AspspInfo {
    pub name: String,
    pub country: String,
    /// Some providers include a logo URL.
    pub logo_url: Option<String>,
}
