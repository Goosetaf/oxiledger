use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Standard chart-of-accounts categories for double-entry bookkeeping.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "server", derive(sqlx::Type))]
#[cfg_attr(
    feature = "server",
    sqlx(type_name = "account_type", rename_all = "PascalCase")
)]
pub enum AccountType {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
}

impl AccountType {
    /// Returns the conventional normal balance side for the account type.
    pub fn default_normal_balance(self) -> NormalBalance {
        match self {
            AccountType::Asset | AccountType::Expense => NormalBalance::Debit,
            AccountType::Liability | AccountType::Equity | AccountType::Revenue => {
                NormalBalance::Credit
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AccountType::Asset => "Asset",
            AccountType::Liability => "Liability",
            AccountType::Equity => "Equity",
            AccountType::Revenue => "Revenue",
            AccountType::Expense => "Expense",
        }
    }
}

impl std::fmt::Display for AccountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Whether the account normally carries a debit or credit balance.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "server", derive(sqlx::Type))]
#[cfg_attr(
    feature = "server",
    sqlx(type_name = "normal_balance", rename_all = "PascalCase")
)]
pub enum NormalBalance {
    Debit,
    Credit,
}

impl NormalBalance {
    pub fn as_str(self) -> &'static str {
        match self {
            NormalBalance::Debit => "Debit",
            NormalBalance::Credit => "Credit",
        }
    }
}

impl std::fmt::Display for NormalBalance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A chart-of-accounts entry belonging to a user.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "server", derive(sqlx::FromRow))]
pub struct Account {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub account_type: AccountType,
    pub normal_balance: NormalBalance,
    pub description: Option<String>,
    pub is_active: bool,
}

/// Request body for creating a new account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateAccountRequest {
    pub name: String,
    pub code: Option<String>,
    pub account_type: AccountType,
    pub description: Option<String>,
}
