use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Whether a journal entry line is a debit or credit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", derive(sqlx::Type))]
#[cfg_attr(
    feature = "server",
    sqlx(type_name = "entry_type", rename_all = "PascalCase")
)]
pub enum EntryType {
    Debit,
    Credit,
}

impl EntryType {
    pub fn as_str(self) -> &'static str {
        match self {
            EntryType::Debit => "Debit",
            EntryType::Credit => "Credit",
        }
    }
}

impl std::fmt::Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single debit or credit line within a transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
pub struct JournalEntry {
    pub id: Uuid,
    pub transaction_id: Uuid,
    pub account_id: Uuid,
    pub account_name: Option<String>,
    pub account_code: Option<String>,
    pub entry_type: EntryType,
    pub amount: Decimal,
    pub memo: Option<String>,
}

/// A transaction header with its journal entry lines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Transaction {
    pub id: Uuid,
    pub user_id: Uuid,
    pub date: chrono::NaiveDate,
    pub description: String,
    pub reference: Option<String>,
    pub entries: Vec<JournalEntry>,
}

/// A transaction header row (no entries, used for list queries).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
pub struct TransactionRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub date: chrono::NaiveDate,
    pub description: String,
    pub reference: Option<String>,
}

/// Input for a single journal entry line when creating a transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JournalEntryInput {
    pub account_id: Uuid,
    pub entry_type: EntryType,
    pub amount: Decimal,
    pub memo: Option<String>,
}

/// Request body for creating a new balanced transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateTransactionRequest {
    pub date: chrono::NaiveDate,
    pub description: String,
    pub reference: Option<String>,
    pub entries: Vec<JournalEntryInput>,
}

impl CreateTransactionRequest {
    /// Returns `true` if the sum of debits equals the sum of credits.
    pub fn is_balanced(&self) -> bool {
        let mut debits = Decimal::ZERO;
        let mut credits = Decimal::ZERO;
        for entry in &self.entries {
            match entry.entry_type {
                EntryType::Debit => debits += entry.amount,
                EntryType::Credit => credits += entry.amount,
            }
        }
        debits == credits
    }

    /// Returns the sum of debit amounts minus credit amounts (should be zero when balanced).
    pub fn imbalance(&self) -> Decimal {
        let mut total = Decimal::ZERO;
        for entry in &self.entries {
            match entry.entry_type {
                EntryType::Debit => total += entry.amount,
                EntryType::Credit => total -= entry.amount,
            }
        }
        total
    }
}

/// A summary of an account's running balance for display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountBalance {
    pub account_id: Uuid,
    pub account_name: String,
    pub account_code: Option<String>,
    pub balance: Decimal,
}
