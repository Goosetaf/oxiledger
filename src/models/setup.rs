use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::account::AccountType;

/// An account the user already has, as far as the setup wizard needs to know.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExistingAccount {
    pub name: String,
    pub account_type: AccountType,
}

/// Everything the setup wizard needs to render its first screen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetupContext {
    pub existing_accounts: Vec<ExistingAccount>,
    /// Ids of the bank sync providers configured on this server, e.g. "gocardless".
    pub bank_sync_providers: Vec<String>,
}

/// One account the setup wizard should create.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StarterAccountInput {
    pub name: String,
    pub code: Option<String>,
    pub account_type: AccountType,
    pub description: Option<String>,
}

/// A first, manually tracked bank account created by the setup wizard.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetupBankAccountInput {
    pub name: String,
    pub currency: String,
    /// Name of the asset account to link it to; `None` leaves it unlinked.
    pub linked_account_name: Option<String>,
}

/// Everything the setup wizard collected, applied in one go.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetupRequest {
    pub accounts: Vec<StarterAccountInput>,
    pub bank_account: Option<SetupBankAccountInput>,
}

/// What applying a [`SetupRequest`] did.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetupResult {
    pub created_accounts: u32,
    /// Accounts left alone because one with the same name and type already existed.
    pub skipped_accounts: u32,
    pub bank_account_id: Option<Uuid>,
}

/// Upper bound on accounts a single setup request may create.
#[cfg(any(feature = "server", test))]
pub const MAX_SETUP_ACCOUNTS: usize = 100;

/// Normalised key used to decide whether two accounts are "the same" during setup.
pub fn account_key(name: &str, account_type: AccountType) -> (String, AccountType) {
    (name.trim().to_lowercase(), account_type)
}

/// An account offered by the setup wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StarterAccount {
    /// Stable identifier used by the templates and the wizard's selection state.
    pub key: &'static str,
    pub name: &'static str,
    pub code: &'static str,
    pub account_type: AccountType,
    pub description: &'static str,
}

const fn starter(
    key: &'static str,
    name: &'static str,
    code: &'static str,
    account_type: AccountType,
    description: &'static str,
) -> StarterAccount {
    StarterAccount {
        key,
        name,
        code,
        account_type,
        description,
    }
}

/// Every account the setup wizard can create, ordered by code.
pub const STARTER_ACCOUNTS: &[StarterAccount] = &[
    // Assets
    starter(
        "checking",
        "Checking Account",
        "1000",
        AccountType::Asset,
        "Everyday bank account",
    ),
    starter(
        "savings",
        "Savings Account",
        "1010",
        AccountType::Asset,
        "Money set aside",
    ),
    starter(
        "cash",
        "Cash",
        "1020",
        AccountType::Asset,
        "Wallet and cash at home",
    ),
    starter(
        "investments",
        "Investments",
        "1100",
        AccountType::Asset,
        "Funds and stocks",
    ),
    starter(
        "retirement",
        "Retirement Savings",
        "1110",
        AccountType::Asset,
        "Pension savings",
    ),
    // Liabilities
    starter(
        "credit_card",
        "Credit Card",
        "2000",
        AccountType::Liability,
        "Credit card balance",
    ),
    starter(
        "mortgage",
        "Mortgage",
        "2100",
        AccountType::Liability,
        "Home loan",
    ),
    starter(
        "loans",
        "Loans",
        "2110",
        AccountType::Liability,
        "Car, student and personal loans",
    ),
    // Equity
    starter(
        "opening_balances",
        "Opening Balances",
        "3000",
        AccountType::Equity,
        "Starting balances",
    ),
    // Revenue
    starter("salary", "Salary", "4000", AccountType::Revenue, "Wages"),
    starter(
        "interest",
        "Interest & Dividends",
        "4010",
        AccountType::Revenue,
        "Returns on savings and investments",
    ),
    starter(
        "other_income",
        "Other Income",
        "4090",
        AccountType::Revenue,
        "Gifts and refunds",
    ),
    // Expenses
    starter(
        "housing",
        "Housing",
        "5000",
        AccountType::Expense,
        "Rent or mortgage payments",
    ),
    starter(
        "utilities",
        "Utilities",
        "5010",
        AccountType::Expense,
        "Electricity, water, internet and phone",
    ),
    starter(
        "groceries",
        "Groceries",
        "5020",
        AccountType::Expense,
        "Food and household supplies",
    ),
    starter(
        "dining",
        "Dining Out",
        "5030",
        AccountType::Expense,
        "Restaurants, cafés and takeaway",
    ),
    starter(
        "transport",
        "Transport",
        "5040",
        AccountType::Expense,
        "Fuel, public transport and parking",
    ),
    starter(
        "health",
        "Health",
        "5050",
        AccountType::Expense,
        "Doctor, pharmacy and fitness",
    ),
    starter(
        "insurance",
        "Insurance",
        "5060",
        AccountType::Expense,
        "Home, car, health and life insurance",
    ),
    starter(
        "subscriptions",
        "Subscriptions",
        "5070",
        AccountType::Expense,
        "Streaming, apps and memberships",
    ),
    starter(
        "entertainment",
        "Entertainment",
        "5080",
        AccountType::Expense,
        "Hobbies, events and going out",
    ),
    starter(
        "shopping",
        "Shopping",
        "5090",
        AccountType::Expense,
        "Clothing and electronics",
    ),
    starter(
        "travel",
        "Travel",
        "5100",
        AccountType::Expense,
        "Trips, flights and accommodation",
    ),
    starter(
        "education",
        "Education",
        "5110",
        AccountType::Expense,
        "Tuition, courses and books",
    ),
    starter(
        "childcare",
        "Childcare",
        "5120",
        AccountType::Expense,
        "Daycare, school and activities",
    ),
    starter(
        "home_maintenance",
        "Home Maintenance",
        "5130",
        AccountType::Expense,
        "Repairs, furniture and garden",
    ),
    starter(
        "gifts",
        "Gifts & Donations",
        "5140",
        AccountType::Expense,
        "Presents and charity",
    ),
    starter(
        "pets",
        "Pets",
        "5150",
        AccountType::Expense,
        "Food, vet and care",
    ),
    starter(
        "taxes",
        "Taxes",
        "5200",
        AccountType::Expense,
        "Income tax and other taxes",
    ),
    starter(
        "bank_fees",
        "Bank Fees",
        "5210",
        AccountType::Expense,
        "Account fees and interest charges",
    ),
    starter(
        "miscellaneous",
        "Miscellaneous",
        "5900",
        AccountType::Expense,
        "Everything else",
    ),
];

/// A ready-made selection of [`STARTER_ACCOUNTS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub recommended: bool,
    /// Keys into [`STARTER_ACCOUNTS`].
    pub accounts: &'static [&'static str],
}

/// The templates offered by the setup wizard, simplest first.
pub const ACCOUNT_TEMPLATES: &[AccountTemplate] = &[
    AccountTemplate {
        id: "empty",
        name: "Empty",
        description: "Start with no accounts",
        recommended: false,
        accounts: &[],
    },
    AccountTemplate {
        id: "essentials",
        name: "Essentials",
        description: "Just the basics",
        recommended: false,
        accounts: &[
            "checking",
            "savings",
            "cash",
            "credit_card",
            "opening_balances",
            "salary",
            "other_income",
            "housing",
            "utilities",
            "groceries",
            "transport",
            "miscellaneous",
        ],
    },
    AccountTemplate {
        id: "personal",
        name: "Personal",
        description: "Everyday personal finances",
        recommended: true,
        accounts: &[
            "checking",
            "savings",
            "cash",
            "credit_card",
            "opening_balances",
            "salary",
            "interest",
            "other_income",
            "housing",
            "utilities",
            "groceries",
            "dining",
            "transport",
            "health",
            "insurance",
            "subscriptions",
            "entertainment",
            "shopping",
            "travel",
            "bank_fees",
            "miscellaneous",
        ],
    },
    AccountTemplate {
        id: "household",
        name: "Household",
        description: "Family, home and loans",
        recommended: false,
        accounts: &[
            "checking",
            "savings",
            "cash",
            "investments",
            "retirement",
            "credit_card",
            "mortgage",
            "loans",
            "opening_balances",
            "salary",
            "interest",
            "other_income",
            "housing",
            "utilities",
            "groceries",
            "dining",
            "transport",
            "health",
            "insurance",
            "subscriptions",
            "entertainment",
            "shopping",
            "travel",
            "education",
            "childcare",
            "home_maintenance",
            "gifts",
            "pets",
            "taxes",
            "bank_fees",
            "miscellaneous",
        ],
    },
];

/// Id of the template preselected when the wizard opens.
pub const DEFAULT_TEMPLATE_ID: &str = "personal";

/// The name each account type goes by in setup, in code order.
pub const ACCOUNT_GROUPS: &[(AccountType, &str)] = &[
    (AccountType::Asset, "Assets"),
    (AccountType::Liability, "Debts"),
    (AccountType::Equity, "Equity"),
    (AccountType::Revenue, "Income"),
    (AccountType::Expense, "Expenses"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn starter_account_keys_names_and_codes_are_unique() {
        let mut keys = HashSet::new();
        let mut codes = HashSet::new();
        let mut names = HashSet::new();

        for account in STARTER_ACCOUNTS {
            assert!(keys.insert(account.key), "duplicate key {}", account.key);
            assert!(
                codes.insert(account.code),
                "duplicate code {}",
                account.code
            );
            assert!(
                names.insert(account_key(account.name, account.account_type)),
                "duplicate name {}",
                account.name
            );
            assert!(account.code.len() <= 20, "code too long for the schema");
        }
    }

    #[test]
    fn templates_only_reference_known_accounts() {
        let keys: HashSet<_> = STARTER_ACCOUNTS.iter().map(|a| a.key).collect();
        let mut template_ids = HashSet::new();

        for template in ACCOUNT_TEMPLATES {
            assert!(template_ids.insert(template.id));
            assert!(template.accounts.len() <= MAX_SETUP_ACCOUNTS);

            let mut seen = HashSet::new();
            for key in template.accounts {
                assert!(keys.contains(key), "{} uses unknown {key}", template.id);
                assert!(seen.insert(key), "{} repeats {key}", template.id);
            }
        }

        assert!(template_ids.contains(DEFAULT_TEMPLATE_ID));
        assert_eq!(
            ACCOUNT_TEMPLATES.iter().filter(|t| t.recommended).count(),
            1
        );
    }

    #[test]
    fn templates_have_distinct_account_sets() {
        let mut sets = HashSet::new();
        for template in ACCOUNT_TEMPLATES {
            let mut accounts = template.accounts.to_vec();
            accounts.sort_unstable();
            assert!(
                sets.insert(accounts),
                "{} duplicates a template",
                template.id
            );
        }
    }

    #[test]
    fn every_account_type_has_a_group() {
        for account in STARTER_ACCOUNTS {
            assert!(ACCOUNT_GROUPS
                .iter()
                .any(|(account_type, _)| *account_type == account.account_type));
        }
    }

    #[test]
    fn account_key_ignores_case_and_surrounding_whitespace() {
        assert_eq!(
            account_key("  Checking Account ", AccountType::Asset),
            account_key("checking account", AccountType::Asset)
        );
        assert_ne!(
            account_key("Taxes", AccountType::Expense),
            account_key("Taxes", AccountType::Liability)
        );
    }
}
