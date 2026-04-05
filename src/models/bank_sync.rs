use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A bank connection record stored in `bank_connections`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
pub struct BankConnection {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider_id: String,
    pub provider_session_id: String,
    pub aspsp_name: Option<String>,
    pub aspsp_country: Option<String>,
    pub access_valid_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl BankConnection {
    /// Returns `true` if the consent / session has expired.
    pub fn is_expired(&self) -> bool {
        match self.access_valid_until {
            Some(until) => Utc::now() >= until,
            None => false,
        }
    }
}

/// A bank account record stored in `bank_accounts`, joined with connection details.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
pub struct BankAccountRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    /// `None` for manually-created accounts.
    pub bank_connection_id: Option<Uuid>,
    pub internal_account_id: Uuid,
    /// `None` for manually-created accounts.
    pub provider_account_uid: Option<String>,
    pub iban: Option<String>,
    pub name: Option<String>,
    pub currency: String,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// A bank account joined with its connection, plus the internal account name —
/// used for the list page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BankAccountSummary {
    pub id: Uuid,
    pub name: Option<String>,
    pub iban: Option<String>,
    pub currency: String,
    pub internal_account_id: Uuid,
    pub internal_account_name: String,
    pub aspsp_name: Option<String>,
    pub aspsp_country: Option<String>,
    /// `None` for manually-created accounts.
    pub provider_id: Option<String>,
    pub access_valid_until: Option<DateTime<Utc>>,
    pub last_synced_at: Option<DateTime<Utc>>,
    /// `true` when the account was added without an external provider.
    pub is_manual: bool,
}

impl BankAccountSummary {
    pub fn is_expired(&self) -> bool {
        if self.is_manual {
            return false;
        }
        match self.access_valid_until {
            Some(until) => Utc::now() >= until,
            None => false,
        }
    }

    pub fn display_name(&self) -> &str {
        self.name
            .as_deref()
            .or(self.iban.as_deref())
            .unwrap_or("Unnamed account")
    }
}

/// Status of an imported bank transaction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BankTransactionStatus {
    Pending,
    Linked,
    Dismissed,
}

impl BankTransactionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            BankTransactionStatus::Pending => "pending",
            BankTransactionStatus::Linked => "linked",
            BankTransactionStatus::Dismissed => "dismissed",
        }
    }
}

impl std::fmt::Display for BankTransactionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An imported (staged) bank transaction stored in `bank_transactions`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
pub struct BankTransaction {
    pub id: Uuid,
    pub bank_account_id: Uuid,
    pub user_id: Uuid,
    pub external_id: String,
    pub date: NaiveDate,
    /// Signed amount: positive = money in (inflow), negative = money out (outflow).
    pub amount: Decimal,
    pub currency: String,
    pub description: String,
    pub reference: Option<String>,
    /// "pending" | "linked" | "dismissed"
    pub status: String,
    pub linked_journal_entry_id: Option<Uuid>,
    pub imported_at: DateTime<Utc>,
}

impl BankTransaction {
    pub fn is_pending(&self) -> bool {
        self.status == "pending"
    }

    pub fn is_inflow(&self) -> bool {
        self.amount >= Decimal::ZERO
    }
}

/// Result of a sync operation — how many transactions were imported vs. skipped.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncResult {
    pub new_count: u32,
    pub skipped_count: u32,
}

/// Data needed to pre-fill a new transaction form from a bank transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BankTransactionPrefill {
    pub bank_transaction_id: Uuid,
    pub date: NaiveDate,
    pub amount: Decimal,
    pub description: String,
    pub reference: Option<String>,
    /// The internal ledger account linked to the bank account.
    pub internal_account_id: Uuid,
    pub internal_account_name: String,
    pub currency: String,
}

/// Summary for the bank account detail page: internal balance vs. bank balance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BankAccountBalanceComparison {
    pub bank_account_id: Uuid,
    /// Sum of journal entries on `internal_account_id` (debit sum - credit sum).
    pub internal_balance: Decimal,
    /// The "closing booked" balance from the provider (if available).
    pub bank_balance: Option<Decimal>,
    pub bank_balance_currency: Option<String>,
    /// Difference: `bank_balance - internal_balance` (positive = bank has more).
    pub difference: Option<Decimal>,
}

/// Minimal info about a bank account as returned by the provider —
/// used on the account-mapping page before the user links it to a ledger account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderBankAccountInfo {
    pub uid: String,
    pub name: Option<String>,
    pub iban: Option<String>,
    pub currency: String,
}

/// An ASPSP (bank) as known to a provider — safe to send to the client.
#[cfg(feature = "server")]
pub use crate::bank_sync::provider::AspspInfo;

/// Client-side stub for `AspspInfo` (used when the server feature is off).
#[cfg(not(feature = "server"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AspspInfo {
    pub institution_id: Option<String>,
    pub name: String,
    pub country: String,
    pub logo_url: Option<String>,
}
