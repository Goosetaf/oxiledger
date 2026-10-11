//! The household that `cargo seed` writes. It is realistic data to develop and
//! test against.
//!
//! This module only describes the data. It lists ledger accounts, bank accounts
//! and transactions for a given "today" and knows nothing about the database.
//! `main.rs` turns it into rows.
//!
//! Every date is relative to "today". There are a few months of booked history,
//! a handful of bank transactions from the last days that still wait to be
//! posted, and a pending transfer dated today. Seeding again moves all of it
//! forward.

use std::collections::HashMap;

use chrono::{Datelike, Days, Months, NaiveDate, TimeDelta};
use oxiledger::{
    bank_sync::provider::ProviderBankAccount,
    models::{account::AccountType, bank_sync::BankTransactionStatus, transaction::EntryType},
};
use rust_decimal::Decimal;

/// Full months of history before the current month.
const HISTORY_MONTHS: u32 = 5;
/// Recurring bank activity is booked up to this many days ago. Anything newer
/// comes from [`Builder::recent_activity`] and is left for the user to post.
const UNBOOKED_DAYS: u64 = 4;
/// The credit card's consent has lapsed, so its history stops at the last sync.
const CARD_LAST_SYNC_DAYS: u64 = 12;

/// What the student loan stood at when the books were opened.
const STUDENT_LOAN_OPENING: i64 = 6400;

// Ledger accounts.
const CHECKING: &str = "checking";
const SAVINGS: &str = "savings";
const CASH: &str = "cash";
const OLD_SAVINGS: &str = "old-savings";
const RECEIVABLES: &str = "receivables";
const CREDIT_CARD: &str = "credit-card";
const STUDENT_LOAN: &str = "student-loan";
const OPENING_BALANCES: &str = "opening-balances";
const SALARY: &str = "salary";
const INTEREST_INCOME: &str = "interest-income";
const OTHER_INCOME: &str = "other-income";
const RENT: &str = "rent";
const GROCERIES: &str = "groceries";
const HOUSEHOLD: &str = "household";
const UTILITIES: &str = "utilities";
const TRANSPORT: &str = "transport";
const DINING: &str = "dining";
const SUBSCRIPTIONS: &str = "subscriptions";
const INSURANCE: &str = "insurance";
const HEALTH: &str = "health";
const ENTERTAINMENT: &str = "entertainment";
const GYM: &str = "gym";
const TRAVEL: &str = "travel";
const BANK_FEES: &str = "bank-fees";
const LOAN_INTEREST: &str = "loan-interest";
const INCOME_TAX: &str = "income-tax";
const PENSION: &str = "pension";
const HOME_IMPROVEMENT: &str = "home-improvement";
const GIFTS: &str = "gifts";

// Bank accounts.
const EVERYDAY: &str = "nordea-everyday";
const NORDEA_SAVINGS: &str = "nordea-savings";
const NORDEA_UNMAPPED: &str = "nordea-unmapped";
const VISA: &str = "norwegian-visa";
const WALLET: &str = "cash-wallet";
const HOLIDAY_FUND: &str = "holiday-fund";

const EVERYDAY_IBAN: &str = "FI2112345600000785";
const SAVINGS_IBAN: &str = "FI0315723500048231";
const UNMAPPED_IBAN: &str = "FI2615723500091467";

// Bank connections.
const NORDEA: &str = "nordea";
const NORWEGIAN: &str = "norwegian";
const NORDEA_RENEWAL: &str = "nordea-renewal";
const HANDELSBANKEN: &str = "handelsbanken";
const DANSKE: &str = "danske";

/// Connections that have just come back from the bank and are waiting on the
/// account-mapping page, with what each one shows there. The app only opens
/// that page after a provider callback, which needs real credentials, so the
/// seeder prints their URLs instead.
pub const MAPPING_CONNECTIONS: [(&str, &str); 3] = [
    (NORDEA_RENEWAL, "new connection, one account already linked"),
    (HANDELSBANKEN, "started from a manual bank account"),
    (DANSKE, "the bank returned no accounts"),
];

pub struct LedgerAccount {
    pub key: &'static str,
    pub code: Option<&'static str>,
    pub name: &'static str,
    pub account_type: AccountType,
    pub description: Option<&'static str>,
    pub is_active: bool,
}

pub struct BankConnection {
    pub key: &'static str,
    pub provider_id: &'static str,
    pub aspsp_name: &'static str,
    pub aspsp_country: &'static str,
    /// Consent expiry relative to the time of seeding. Negative means expired.
    pub access_valid_for: TimeDelta,
    pub provider_accounts: Vec<ProviderBankAccount>,
    /// The manual bank account this connection was started from, if any.
    pub source_bank_account: Option<&'static str>,
}

pub struct BankAccount {
    pub key: &'static str,
    pub name: Option<&'static str>,
    pub iban: Option<&'static str>,
    pub currency: &'static str,
    pub ledger_account: Option<&'static str>,
    /// `None` for manual accounts.
    pub connection: Option<ConnectedAccount>,
    /// Statement balance before the first seeded bank transaction.
    pub opening_balance: Decimal,
    /// Part of the bank's balance that never arrived as a transaction, which
    /// leaves a difference the ledger cannot explain.
    pub unreported: Decimal,
}

pub struct ConnectedAccount {
    pub connection: &'static str,
    pub provider_account_uid: &'static str,
    /// How long before seeding the account was last synced, or `None` for never.
    pub last_synced_ago: Option<TimeDelta>,
}

pub struct BankTxn {
    pub bank_account: &'static str,
    pub date: NaiveDate,
    /// Positive for an inflow and negative for an outflow.
    pub amount: Decimal,
    pub description: String,
    pub reference: Option<String>,
    pub status: BankTransactionStatus,
}

pub struct Line {
    pub account: &'static str,
    pub entry_type: EntryType,
    pub amount: Decimal,
    pub memo: Option<String>,
    /// Index into [`Dataset::bank_transactions`] when this line books a bank transaction.
    pub bank_txn: Option<usize>,
}

impl Line {
    fn debit(account: &'static str, amount: Decimal) -> Self {
        Self {
            account,
            entry_type: EntryType::Debit,
            amount,
            memo: None,
            bank_txn: None,
        }
    }

    fn credit(account: &'static str, amount: Decimal) -> Self {
        Self {
            entry_type: EntryType::Credit,
            ..Self::debit(account, amount)
        }
    }

    fn memo(mut self, memo: &str) -> Self {
        self.memo = Some(memo.to_string());
        self
    }
}

pub struct JournalTxn {
    pub date: NaiveDate,
    pub description: String,
    pub reference: Option<String>,
    pub lines: Vec<Line>,
}

pub struct Dataset {
    pub accounts: Vec<LedgerAccount>,
    pub bank_connections: Vec<BankConnection>,
    pub bank_accounts: Vec<BankAccount>,
    pub bank_transactions: Vec<BankTxn>,
    pub transactions: Vec<JournalTxn>,
}

impl Dataset {
    /// Builds the household as it looks on `today`.
    pub fn household(today: NaiveDate) -> Self {
        let start =
            today.with_day(1).expect("every month has a first day") - Months::new(HISTORY_MONTHS);

        let mut builder = Builder {
            data: Self {
                accounts: chart_of_accounts(),
                bank_connections: bank_connections(),
                bank_accounts: bank_accounts(),
                bank_transactions: Vec::new(),
                transactions: Vec::new(),
            },
            today,
            rng: Rng(0x5eed_5eed_5eed_5eed),
            card_activity: HashMap::new(),
            loan_balance: eur(STUDENT_LOAN_OPENING, 0),
        };

        builder.opening_balances(start);
        for index in 0..=HISTORY_MONTHS {
            builder.month(index, start + Months::new(index));
        }
        builder.recent_activity();

        builder.data
    }

    pub fn account(&self, key: &str) -> &LedgerAccount {
        self.accounts
            .iter()
            .find(|account| account.key == key)
            .unwrap_or_else(|| panic!("unknown ledger account '{key}'"))
    }

    pub fn bank_account(&self, key: &str) -> &BankAccount {
        self.bank_accounts
            .iter()
            .find(|account| account.key == key)
            .unwrap_or_else(|| panic!("unknown bank account '{key}'"))
    }

    /// The balance the bank would report. It covers everything on the statement,
    /// whether or not it has been booked in the ledger yet.
    pub fn bank_balance(&self, key: &str) -> Decimal {
        let bank_account = self.bank_account(key);
        self.bank_transactions
            .iter()
            .filter(|txn| txn.bank_account == key)
            .fold(
                bank_account.opening_balance + bank_account.unreported,
                |sum, txn| sum + txn.amount,
            )
    }

    pub fn bank_transaction_count(&self, status: BankTransactionStatus) -> usize {
        self.bank_transactions
            .iter()
            .filter(|txn| txn.status == status)
            .count()
    }
}

#[rustfmt::skip]
fn chart_of_accounts() -> Vec<LedgerAccount> {
    use AccountType::{Asset, Equity, Expense, Liability, Revenue};

    let account = |key, code, name, account_type, description| LedgerAccount {
        key,
        code,
        name,
        account_type,
        description,
        is_active: true,
    };
    let inactive = |key, code, name, account_type, description| LedgerAccount {
        is_active: false,
        ..account(key, code, name, account_type, description)
    };

    vec![
        account(CHECKING, Some("1000"), "Checking Account", Asset, Some("Everyday account")),
        account(SAVINGS, Some("1010"), "Savings Account", Asset, Some("Emergency fund")),
        account(CASH, Some("1020"), "Cash", Asset, Some("Wallet")),
        inactive(OLD_SAVINGS, Some("1090"), "Old Savings Account", Asset, Some("Closed")),
        account(RECEIVABLES, Some("1200"), "Receivables", Asset, Some("Money friends owe me")),
        account(CREDIT_CARD, Some("2000"), "Credit Card", Liability, Some("Bank Norwegian Visa")),
        account(STUDENT_LOAN, Some("2100"), "Student Loan", Liability, None),
        account(OPENING_BALANCES, Some("3000"), "Opening Balances", Equity, None),
        account(SALARY, Some("4000"), "Salary", Revenue, Some("Gross salary before tax")),
        account(INTEREST_INCOME, Some("4100"), "Interest Income", Revenue, None),
        account(OTHER_INCOME, Some("4900"), "Other Income", Revenue, Some("Refunds and one-offs")),
        account(RENT, Some("5000"), "Rent", Expense, None),
        account(GROCERIES, Some("5100"), "Groceries", Expense, None),
        account(HOUSEHOLD, Some("5150"), "Household Goods", Expense, None),
        account(UTILITIES, Some("5200"), "Utilities", Expense, Some("Power, internet, phone")),
        account(TRANSPORT, Some("5300"), "Transport", Expense, None),
        account(DINING, Some("5400"), "Dining Out", Expense, None),
        account(SUBSCRIPTIONS, Some("5500"), "Subscriptions", Expense, None),
        account(INSURANCE, Some("5600"), "Insurance", Expense, Some("Billed quarterly")),
        account(HEALTH, Some("5700"), "Health & Personal Care", Expense, None),
        account(ENTERTAINMENT, Some("5800"), "Entertainment", Expense, None),
        inactive(GYM, Some("5810"), "Gym Membership", Expense, Some("Cancelled")),
        account(TRAVEL, Some("5850"), "Travel", Expense, None),
        account(BANK_FEES, Some("5900"), "Bank Fees", Expense, None),
        account(LOAN_INTEREST, Some("5910"), "Loan Interest", Expense, None),
        account(INCOME_TAX, Some("5950"), "Income Tax", Expense, Some("Withheld from salary")),
        account(PENSION, Some("5960"), "Pension Contributions", Expense, None),
        // Never used, so it is the one account that can be deleted.
        account(HOME_IMPROVEMENT, Some("5990"), "Home Improvement", Expense, None),
        account(GIFTS, None, "Gifts & Donations", Expense, Some("No account code")),
    ]
}

fn bank_connections() -> Vec<BankConnection> {
    let account_in =
        |currency: &str, uid: &str, name: Option<&str>, iban: Option<&str>| ProviderBankAccount {
            uid: uid.to_string(),
            name: name.map(str::to_string),
            iban: iban.map(str::to_string),
            currency: currency.to_string(),
        };
    let account = |uid, name, iban| account_in("EUR", uid, name, iban);
    let connection = |key, provider_id, aspsp_name, aspsp_country, valid_days| BankConnection {
        key,
        provider_id,
        aspsp_name,
        aspsp_country,
        access_valid_for: TimeDelta::days(valid_days),
        provider_accounts: Vec::new(),
        source_bank_account: None,
    };

    vec![
        BankConnection {
            provider_accounts: vec![
                account(
                    "seed-nordea-everyday",
                    Some("Nordea Everyday"),
                    Some(EVERYDAY_IBAN),
                ),
                account(
                    "seed-nordea-savings",
                    Some("Nordea Savings"),
                    Some(SAVINGS_IBAN),
                ),
                account("seed-nordea-unmapped", None, Some(UNMAPPED_IBAN)),
            ],
            ..connection(NORDEA, "enable_banking", "Nordea", "FI", 76)
        },
        // The consent ran out ten days ago.
        BankConnection {
            provider_accounts: vec![account(
                "seed-norwegian-visa",
                Some("Bank Norwegian Visa"),
                None,
            )],
            ..connection(NORWEGIAN, "gocardless", "Bank Norwegian", "FI", -10)
        },
        // A renewed Nordea consent. One account is already linked through the
        // connection above, the other two are new, and one of those has neither
        // a name nor an IBAN.
        BankConnection {
            provider_accounts: vec![
                account(
                    "seed-nordea-everyday",
                    Some("Nordea Everyday"),
                    Some(EVERYDAY_IBAN),
                ),
                account(
                    "seed-nordea-joint",
                    Some("Nordea Joint Account"),
                    Some("FI9415723500127394"),
                ),
                account("seed-nordea-unnamed", None, None),
            ],
            ..connection(NORDEA_RENEWAL, "enable_banking", "Nordea", "FI", 90)
        },
        BankConnection {
            provider_accounts: vec![
                account_in(
                    "SEK",
                    "seed-handelsbanken-allkonto",
                    Some("Allkonto"),
                    Some("SE7660000000000482913576"),
                ),
                account_in(
                    "SEK",
                    "seed-handelsbanken-sparkonto",
                    Some("Sparkonto"),
                    Some("SE9860000000000719305284"),
                ),
            ],
            source_bank_account: Some(HOLIDAY_FUND),
            ..connection(HANDELSBANKEN, "gocardless", "Handelsbanken", "SE", 90)
        },
        connection(DANSKE, "enable_banking", "Danske Bank", "FI", 90),
    ]
}

fn bank_accounts() -> Vec<BankAccount> {
    let connected = |connection, provider_account_uid, last_synced_ago| {
        Some(ConnectedAccount {
            connection,
            provider_account_uid,
            last_synced_ago,
        })
    };
    let account = |key, name, iban, ledger_account, connection, opening_balance| BankAccount {
        key,
        name,
        iban,
        currency: "EUR",
        ledger_account,
        connection,
        opening_balance,
        unreported: Decimal::ZERO,
    };
    let synced = Some(TimeDelta::hours(2));
    let card_synced = Some(TimeDelta::days(CARD_LAST_SYNC_DAYS as i64 - 1));

    vec![
        account(
            EVERYDAY,
            Some("Nordea Everyday"),
            Some(EVERYDAY_IBAN),
            Some(CHECKING),
            connected(NORDEA, "seed-nordea-everyday", synced),
            eur(3480, 55),
        ),
        account(
            NORDEA_SAVINGS,
            Some("Nordea Savings"),
            Some(SAVINGS_IBAN),
            Some(SAVINGS),
            connected(NORDEA, "seed-nordea-savings", synced),
            eur(6200, 0),
        ),
        // Mapped without a ledger account and never synced. The bank returned no name.
        account(
            NORDEA_UNMAPPED,
            None,
            Some(UNMAPPED_IBAN),
            None,
            connected(NORDEA, "seed-nordea-unmapped", None),
            Decimal::ZERO,
        ),
        // The bank's balance includes a currency fee it never sent as a transaction.
        BankAccount {
            unreported: -eur(4, 0),
            ..account(
                VISA,
                Some("Bank Norwegian Visa"),
                None,
                Some(CREDIT_CARD),
                connected(NORWEGIAN, "seed-norwegian-visa", card_synced),
                -eur(318, 40),
            )
        },
        account(
            WALLET,
            Some("Cash Wallet"),
            None,
            Some(CASH),
            None,
            eur(45, 0),
        ),
        // Not linked to a ledger account, and kept in another currency.
        BankAccount {
            currency: "SEK",
            ..account(
                HOLIDAY_FUND,
                Some("Joint Holiday Fund"),
                Some("SE4550000000058398257466"),
                None,
                None,
                Decimal::ZERO,
            )
        },
    ]
}

struct Builder {
    data: Dataset,
    today: NaiveDate,
    rng: Rng,
    /// Card purchases less refunds per (year, month). The next month's invoice pays it off.
    card_activity: HashMap<(i32, u32), Decimal>,
    /// What is still owed on the student loan. Each month's interest depends on it.
    loan_balance: Decimal,
}

impl Builder {
    fn opening_balances(&mut self, date: NaiveDate) {
        let old_savings = eur(1250, 0);
        let student_loan = self.loan_balance;

        let mut lines = Vec::new();
        let mut equity = old_savings - student_loan;
        for bank_account in &self.data.bank_accounts {
            let (Some(account), balance) =
                (bank_account.ledger_account, bank_account.opening_balance)
            else {
                continue;
            };
            equity += balance;
            lines.push(if balance >= Decimal::ZERO {
                Line::debit(account, balance)
            } else {
                Line::credit(account, -balance)
            });
        }
        lines.push(Line::debit(OLD_SAVINGS, old_savings));
        lines.push(Line::credit(STUDENT_LOAN, student_loan));
        lines.push(
            Line::credit(OPENING_BALANCES, equity).memo("Net worth when the books were opened"),
        );

        self.journal(date, "Opening balances", Some("OPENING"), lines);
        self.post(
            NORDEA_SAVINGS,
            date + Days::new(2),
            old_savings,
            "Transfer from closed savings account",
            None,
            OLD_SAVINGS,
        );
    }

    /// One month of recurring household activity. `index` counts from the
    /// first month of history and `first` is the first day of the month.
    #[rustfmt::skip]
    fn month(&mut self, index: u32, first: NaiveDate) {
        let day = |day: u32| first.with_day(day).expect("only days up to the 28th are used");
        let period = first.format("%Y-%m").to_string();
        let reference = |prefix: &str| format!("{prefix} {period}");

        // Fixed costs out of the everyday account.
        let electricity = match first.month() {
            11 | 12 | 1 | 2 | 3 => self.rng.eur(92, 134),
            _ => self.rng.eur(46, 78),
        };
        let (rent, power, phone) = (reference("RENT"), reference("INV 417"), reference("INV 883"));
        self.post(EVERYDAY, day(1), -eur(1150, 0), "Kiinteistö Oy Aurinkorinne", Some(&rent), RENT);
        self.post(EVERYDAY, day(1), -eur(73, 60), "HSL monthly ticket", None, TRANSPORT);
        self.post(EVERYDAY, day(5), -electricity, "Helen Oy", Some(&power), UTILITIES);
        self.post(EVERYDAY, day(8), -eur(39, 90), "Elisa Oyj", Some(&phone), UTILITIES);
        if first.month() % 3 == 1 {
            let policy = reference("POLICY 4471");
            self.post(EVERYDAY, day(10), -eur(186, 30), "If Vakuutus", Some(&policy), INSURANCE);
        }
        self.post(EVERYDAY, day(28), -eur(5, 0), "Nordea service fee", None, BANK_FEES);

        // Groceries, with one bigger shop split across two expense accounts.
        let shops = ["K-Supermarket Kamppi", "S-market Kallio", "Lidl Sörnäinen", "Alepa Töölö"];
        for grocery_day in [3, 7, 11, 18, 22, 26] {
            let shop = self.rng.pick(&shops);
            let amount = self.rng.eur(17, 94);
            self.post(EVERYDAY, day(grocery_day), -amount, shop, None, GROCERIES);
        }
        let food = self.rng.eur(58, 104);
        let household = self.rng.eur(12, 47);
        self.post_split(EVERYDAY, day(14), -(food + household), "Prisma Itäkeskus", None, vec![
            Line::debit(GROCERIES, food).memo("Weekly shop"),
            Line::debit(HOUSEHOLD, household).memo("Cleaning supplies and kitchenware"),
        ]);

        // Salary arrives net. The journal entry records gross pay and deductions.
        let (salary, gross) = (reference("SALARY"), format!("Gross salary {}", first.format("%B")));
        self.post_split(EVERYDAY, day(25), eur(3240, 0), "ACME Oy", Some(&salary), vec![
            Line::credit(SALARY, eur(4650, 0)).memo(&gross),
            Line::debit(INCOME_TAX, eur(1162, 50)).memo("Withheld income tax"),
            Line::debit(PENSION, eur(247, 50)).memo("Employee pension contribution"),
        ]);
        // The loan payment is fixed. Interest takes its share and the rest pays the loan down.
        let (loan, payment) = (reference("LOAN 88-20417"), eur(210, 0));
        let interest = (self.loan_balance * Decimal::new(45, 4)).round_dp(2);
        self.post_split(EVERYDAY, day(28), -payment, "OP student loan", Some(&loan), vec![
            Line::debit(STUDENT_LOAN, payment - interest).memo("Principal"),
            Line::debit(LOAN_INTEREST, interest).memo("Interest"),
        ]);
        self.loan_balance -= payment - interest;

        // Money moving between the household's own accounts. The card invoice
        // pays off whatever was spent on the card the month before.
        let previous = first - Months::new(1);
        let last_month_on_card = self.card_activity.get(&(previous.year(), previous.month()));
        let invoice = match index {
            0 => -self.data.bank_account(VISA).opening_balance,
            _ => last_month_on_card.copied().unwrap_or_default(),
        };
        self.transfer(EVERYDAY, WALLET, day(9), eur(60, 0), "Otto. ATM withdrawal", "Withdrawal");
        self.transfer(EVERYDAY, NORDEA_SAVINGS, day(15), eur(400, 0), "To savings", "Own transfer");
        if invoice > Decimal::ZERO {
            self.transfer(EVERYDAY, VISA, day(20), invoice, "Bank Norwegian", "Payment received");
        }
        let interest = self.rng.eur(2, 4);
        self.post(NORDEA_SAVINGS, day(1), interest, "Interest", None, INTEREST_INCOME);

        // Card spending.
        self.post(VISA, day(2), -eur(15, 0), "Suomen Punainen Risti", None, GIFTS);
        let places = ["Ravintola Juuri", "Fafa's Kallio", "Café Regatta", "Putte's Pizza", "Wolt"];
        for dining_day in [6, 13, 19, 27] {
            let place = self.rng.pick(&places);
            let amount = self.rng.eur(11, 68);
            self.post(VISA, day(dining_day), -amount, place, None, DINING);
        }
        self.post(VISA, day(12), -eur(15, 99), "Netflix", None, SUBSCRIPTIONS);
        self.post(VISA, day(17), -eur(12, 49), "Spotify", None, SUBSCRIPTIONS);
        if index.is_multiple_of(2) {
            let amount = self.rng.eur(8, 36);
            self.post(VISA, day(21), -amount, "Yliopiston Apteekki", None, HEALTH);
        }
        let amount = self.rng.eur(14, 39);
        self.post(VISA, day(23), -amount, "Finnkino Tennispalatsi", None, ENTERTAINMENT);
        if index < 2 {
            // The gym was cancelled after two months and its account deactivated.
            self.post(VISA, day(4), -eur(34, 90), "Elixia Helsinki", None, GYM);
        }

        // Cash spending out of the wallet, entered by hand.
        let amount = self.rng.eur(8, 19);
        self.post(WALLET, day(10), -amount, "Hakaniemi market hall", None, GROCERIES);
        let amount = self.rng.eur(6, 12);
        self.post(WALLET, day(16), -amount, "Coffee and pastry", None, DINING);
        self.post(WALLET, day(24), -eur(28, 0), "Barber", None, HEALTH);

        // One-offs that give individual months some character.
        match index {
            1 => self.post(EVERYDAY, day(17), eur(286, 40), "Tax refund", None, INCOME_TAX),
            2 => {
                self.post(VISA, day(9), -eur(219, 0), "Finnair", Some("AY1331 HEL-ARN"), TRAVEL);
                self.post(EVERYDAY, day(21), eur(120, 0), "Tori.fi bike sale", None, OTHER_INCOME);
            }
            3 => {
                // An inflow on the card, which is a refund credited back to the expense account.
                self.post(VISA, day(4), eur(42, 0), "Finnair refund", Some("AY1331"), TRAVEL);
                // Booked by hand, with no bank transaction behind it.
                self.journal(day(16), "Dinner with Aino, her half", None, vec![
                    Line::debit(RECEIVABLES, eur(32, 0)).memo("Aino owes me"),
                    Line::credit(DINING, eur(32, 0)),
                ]);
            }
            4 => self.post(EVERYDAY, day(6), eur(32, 0), "MobilePay Aino K", None, RECEIVABLES),
            _ => {}
        }
    }

    /// What is on the bank statements but has not been booked yet.
    #[rustfmt::skip]
    fn recent_activity(&mut self) {
        let today = self.today;
        let ago = |days: u64| today - Days::new(days);

        // A matching pair on two bank accounts. The composer suggests posting it as a transfer.
        self.pending(EVERYDAY, today, -eur(60, 0), "Otto. ATM withdrawal", None);
        self.pending(WALLET, today, eur(60, 0), "Withdrawal", None);

        self.pending(EVERYDAY, today, -eur(47, 85), "K-Supermarket Kamppi", None);
        self.pending(EVERYDAY, ago(1), -eur(9, 80), "Fazer Café Kluuvi", None);
        self.pending(EVERYDAY, ago(1), eur(129, 0), "Verkkokauppa.com refund", Some("ORDER 7712"));
        self.pending(EVERYDAY, ago(2), -eur(34, 99), "Clas Ohlson Kamppi", None);
        self.pending(EVERYDAY, ago(2), -eur(18, 20), "Alepa Töölö", None);
        self.pending(EVERYDAY, ago(3), -eur(12, 0), "R-kioski Rautatientori", None);
        // A hotel deposit that was reversed. There is nothing to book, so both were dismissed.
        let booking = Some("RES 55209");
        self.dismissed(EVERYDAY, ago(3), -eur(150, 0), "Scandic Hotels deposit", booking);
        self.dismissed(EVERYDAY, ago(3), eur(150, 0), "Scandic Hotels deposit reversal", booking);
        // Banks send some payments without any text. This one was overlooked, so it is
        // dated on a day that is otherwise booked and can be linked from a journal entry.
        let overlooked = self.last_booked(EVERYDAY);
        self.pending(EVERYDAY, overlooked, -eur(6, 40), "", None);

        self.pending(WALLET, today, -eur(4, 50), "Coffee at the market square", None);
        self.pending(WALLET, ago(2), -eur(15, 0), "Farmers' market", None);
        self.dismissed(WALLET, ago(5), -eur(2, 0), "Parking meter (entered twice)", None);

        // The card's last purchase before its consent ran out was never booked.
        let last_purchase = self.last_booked(VISA);
        self.pending(VISA, last_purchase, -eur(64, 90), "Stockmann Helsinki", None);

        // Not linked to a ledger account, and kept in another currency.
        self.pending(HOLIDAY_FUND, today, Decimal::new(1500, 0), "Monthly deposit", None);
        let tickets = -Decimal::new(1280, 0);
        self.pending(HOLIDAY_FUND, ago(6), tickets, "SJ Stockholm-Åre", Some("BOOKING KXT4821"));
        self.pending(HOLIDAY_FUND, ago(30), Decimal::new(1500, 0), "Monthly deposit", None);
    }

    /// The most recent day with a booked transaction on a bank account.
    fn last_booked(&self, bank_account: &str) -> NaiveDate {
        self.data
            .bank_transactions
            .iter()
            .filter(|txn| txn.bank_account == bank_account)
            .filter(|txn| txn.status == BankTransactionStatus::Linked)
            .map(|txn| txn.date)
            .max()
            .expect("bank accounts with a ledger account have booked history")
    }

    /// The last day on which a bank account's recurring activity is booked.
    fn booked_until(&self, bank_account: &str) -> NaiveDate {
        let days = if bank_account == VISA {
            CARD_LAST_SYNC_DAYS
        } else {
            UNBOOKED_DAYS
        };
        self.today - Days::new(days)
    }

    fn bank_txn(
        &mut self,
        bank_account: &'static str,
        date: NaiveDate,
        amount: Decimal,
        description: &str,
        reference: Option<&str>,
        status: BankTransactionStatus,
    ) -> usize {
        self.data.bank_transactions.push(BankTxn {
            bank_account,
            date,
            amount,
            description: description.to_string(),
            reference: reference.map(str::to_string),
            status,
        });
        self.data.bank_transactions.len() - 1
    }

    fn pending(
        &mut self,
        bank_account: &'static str,
        date: NaiveDate,
        amount: Decimal,
        description: &str,
        reference: Option<&str>,
    ) {
        self.bank_txn(
            bank_account,
            date,
            amount,
            description,
            reference,
            BankTransactionStatus::Pending,
        );
    }

    fn dismissed(
        &mut self,
        bank_account: &'static str,
        date: NaiveDate,
        amount: Decimal,
        description: &str,
        reference: Option<&str>,
    ) {
        self.bank_txn(
            bank_account,
            date,
            amount,
            description,
            reference,
            BankTransactionStatus::Dismissed,
        );
    }

    /// The journal line the app creates for a bank transaction. It is on the mapped
    /// ledger account, a debit for an inflow and a credit for an outflow.
    fn bank_line(&self, bank_txn: usize) -> Line {
        let txn = &self.data.bank_transactions[bank_txn];
        let account = self
            .data
            .bank_account(txn.bank_account)
            .ledger_account
            .expect("only bank accounts with a ledger account have booked transactions");
        let line = if txn.amount >= Decimal::ZERO {
            Line::debit(account, txn.amount)
        } else {
            Line::credit(account, -txn.amount)
        };
        Line {
            bank_txn: Some(bank_txn),
            ..line.memo(&txn.description)
        }
    }

    fn journal(
        &mut self,
        date: NaiveDate,
        description: &str,
        reference: Option<&str>,
        lines: Vec<Line>,
    ) {
        self.data.transactions.push(JournalTxn {
            date,
            description: description.to_string(),
            reference: reference.map(str::to_string),
            lines,
        });
    }

    /// A bank transaction booked against a single other account.
    fn post(
        &mut self,
        bank_account: &'static str,
        date: NaiveDate,
        amount: Decimal,
        description: &str,
        reference: Option<&str>,
        counter_account: &'static str,
    ) {
        let counter_line = if amount >= Decimal::ZERO {
            Line::credit(counter_account, amount)
        } else {
            Line::debit(counter_account, -amount)
        };
        self.post_split(
            bank_account,
            date,
            amount,
            description,
            reference,
            vec![counter_line],
        );
    }

    /// A bank transaction booked against several other lines. Skipped when it
    /// falls after the point up to which the bank account is booked.
    fn post_split(
        &mut self,
        bank_account: &'static str,
        date: NaiveDate,
        amount: Decimal,
        description: &str,
        reference: Option<&str>,
        counter_lines: Vec<Line>,
    ) {
        if date > self.booked_until(bank_account) {
            return;
        }
        if bank_account == VISA {
            *self
                .card_activity
                .entry((date.year(), date.month()))
                .or_default() -= amount;
        }

        let bank_txn = self.bank_txn(
            bank_account,
            date,
            amount,
            description,
            reference,
            BankTransactionStatus::Linked,
        );
        let mut lines = vec![self.bank_line(bank_txn)];
        lines.extend(counter_lines);
        self.journal(date, description, reference, lines);
    }

    /// Money moving between two of the household's bank accounts, as one journal
    /// transaction that books both bank transactions.
    fn transfer(
        &mut self,
        from: &'static str,
        to: &'static str,
        date: NaiveDate,
        amount: Decimal,
        from_description: &str,
        to_description: &str,
    ) {
        if date > self.booked_until(from).min(self.booked_until(to)) {
            return;
        }

        let outflow = self.bank_txn(
            from,
            date,
            -amount,
            from_description,
            None,
            BankTransactionStatus::Linked,
        );
        let inflow = self.bank_txn(
            to,
            date,
            amount,
            to_description,
            None,
            BankTransactionStatus::Linked,
        );
        let lines = vec![self.bank_line(outflow), self.bank_line(inflow)];
        // Same wording as the composer's "Match as transfer".
        let description = format!(
            "Transfer: {} -> {}",
            self.data.account(lines[0].account).name,
            self.data.account(lines[1].account).name
        );
        self.journal(date, &description, None, lines);
    }
}

fn eur(euros: i64, cents: i64) -> Decimal {
    Decimal::new(euros * 100 + cents, 2)
}

/// A tiny xorshift generator. Amounts vary from month to month but the dataset
/// stays reproducible for a given day.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// An amount of at least `min` and less than `max` euros, with cents.
    fn eur(&mut self, min: i64, max: i64) -> Decimal {
        let cents = (max - min) * 100;
        Decimal::new(min * 100 + (self.next() % cents as u64) as i64, 2)
    }

    fn pick<'a>(&mut self, options: &[&'a str]) -> &'a str {
        options[(self.next() % options.len() as u64) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Datasets for every day of a leap year, so month lengths and the
    /// position of "today" within the month cannot hide a problem.
    fn datasets() -> impl Iterator<Item = (NaiveDate, Dataset)> {
        NaiveDate::from_ymd_opt(2024, 1, 1)
            .unwrap()
            .iter_days()
            .take(366)
            .map(|today| (today, Dataset::household(today)))
    }

    fn ledger_balance(data: &Dataset, account: &str) -> Decimal {
        data.transactions
            .iter()
            .flat_map(|txn| &txn.lines)
            .filter(|line| line.account == account)
            .fold(Decimal::ZERO, |sum, line| match line.entry_type {
                EntryType::Debit => sum + line.amount,
                EntryType::Credit => sum - line.amount,
            })
    }

    fn unbooked(data: &Dataset, bank_account: &str) -> Decimal {
        data.bank_transactions
            .iter()
            .filter(|txn| txn.bank_account == bank_account)
            .filter(|txn| txn.status != BankTransactionStatus::Linked)
            .fold(Decimal::ZERO, |sum, txn| sum + txn.amount)
    }

    #[test]
    fn journal_transactions_are_balanced() {
        for (today, data) in datasets() {
            for txn in &data.transactions {
                let name = &txn.description;
                assert!(txn.lines.len() >= 2, "{today}: '{name}' needs two lines");
                assert!(txn.date <= today, "{today}: '{name}' is in the future");
                assert!(
                    !name.is_empty(),
                    "{today}: a transaction has no description"
                );

                let mut imbalance = Decimal::ZERO;
                for line in &txn.lines {
                    assert!(
                        line.amount > Decimal::ZERO,
                        "{today}: '{name}' has an empty line"
                    );
                    data.account(line.account);
                    match line.entry_type {
                        EntryType::Debit => imbalance += line.amount,
                        EntryType::Credit => imbalance -= line.amount,
                    }
                }
                assert_eq!(imbalance, Decimal::ZERO, "{today}: '{name}' is unbalanced");
            }
        }
    }

    /// The rules `validate_linked_bank_transactions` enforces when the app links
    /// a bank transaction to a journal line.
    #[test]
    fn linked_bank_transactions_match_their_journal_line() {
        for (today, data) in datasets() {
            let mut links = vec![0; data.bank_transactions.len()];
            for txn in &data.transactions {
                let name = &txn.description;
                for line in &txn.lines {
                    let Some(index) = line.bank_txn else { continue };
                    let bank_txn = &data.bank_transactions[index];
                    let ledger_account = data.bank_account(bank_txn.bank_account).ledger_account;
                    let inflow = bank_txn.amount >= Decimal::ZERO;
                    links[index] += 1;

                    assert_eq!(bank_txn.date, txn.date, "{today}: '{name}'");
                    assert_eq!(ledger_account, Some(line.account), "{today}: '{name}'");
                    assert_eq!(line.amount, bank_txn.amount.abs(), "{today}: '{name}'");
                    assert_eq!(
                        line.entry_type == EntryType::Debit,
                        inflow,
                        "{today}: '{name}'"
                    );
                }
            }

            for (bank_txn, links) in data.bank_transactions.iter().zip(links) {
                let name = &bank_txn.description;
                let expected = usize::from(bank_txn.status == BankTransactionStatus::Linked);
                assert_eq!(links, expected, "{today}: '{name}'");
                assert!(bank_txn.date <= today, "{today}: '{name}' is in the future");
            }
        }
    }

    /// What each bank account's detail page shows as the difference between the
    /// bank's balance and the ledger.
    #[test]
    fn balance_differences_are_what_the_data_intends() {
        for (today, data) in datasets() {
            let difference = |bank_account: &str| {
                let ledger_account = data.bank_account(bank_account).ledger_account.unwrap();
                data.bank_balance(bank_account) - ledger_balance(&data, ledger_account)
            };

            // Nothing left to book, so the app shows "Balanced".
            assert_eq!(difference(NORDEA_SAVINGS), Decimal::ZERO, "{today}");
            // Explained by the transactions that are still pending or were dismissed.
            for bank_account in [EVERYDAY, WALLET] {
                assert_eq!(
                    difference(bank_account),
                    unbooked(&data, bank_account),
                    "{today}"
                );
                assert_ne!(difference(bank_account), Decimal::ZERO, "{today}");
            }
            // Something remains even once the pending purchase is booked.
            let unexplained = difference(VISA) - unbooked(&data, VISA);
            assert_eq!(unexplained, data.bank_account(VISA).unreported, "{today}");
            assert_ne!(unexplained, Decimal::ZERO, "{today}");
        }
    }

    /// Money never runs out. Asset accounts stay in credit at the end of every
    /// day, and the credit card is never overpaid.
    #[test]
    fn balances_stay_on_the_right_side_of_zero() {
        for (today, data) in datasets() {
            for bank_account in &data.bank_accounts {
                let mut by_day = BTreeMap::new();
                for txn in &data.bank_transactions {
                    if txn.bank_account == bank_account.key {
                        *by_day.entry(txn.date).or_insert(Decimal::ZERO) += txn.amount;
                    }
                }

                let mut balance = bank_account.opening_balance;
                for (date, change) in by_day {
                    balance += change;
                    let key = bank_account.key;
                    if key == VISA {
                        assert!(
                            balance <= Decimal::ZERO,
                            "{today}: {key} is {balance} on {date}"
                        );
                    } else {
                        assert!(
                            balance >= Decimal::ZERO,
                            "{today}: {key} is {balance} on {date}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn references_resolve_and_codes_are_unique() {
        let data = Dataset::household(NaiveDate::from_ymd_opt(2024, 6, 15).unwrap());

        let mut codes: Vec<_> = data
            .accounts
            .iter()
            .filter_map(|account| account.code)
            .collect();
        codes.sort_unstable();
        let count = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), count);

        for bank_account in &data.bank_accounts {
            if let Some(key) = bank_account.ledger_account {
                data.account(key);
            }
            if let Some(connected) = &bank_account.connection {
                let connection = data
                    .bank_connections
                    .iter()
                    .find(|connection| connection.key == connected.connection)
                    .expect("bank account points at a seeded connection");
                assert!(connection
                    .provider_accounts
                    .iter()
                    .any(|account| account.uid == connected.provider_account_uid));
            }
        }
        for connection in &data.bank_connections {
            if let Some(key) = connection.source_bank_account {
                assert!(data.bank_account(key).connection.is_none());
            }
        }
        for txn in &data.bank_transactions {
            data.bank_account(txn.bank_account);
        }
    }

    /// The states the bank account list and the mapping page are meant to show.
    #[test]
    fn bank_accounts_and_connections_cover_their_states() {
        let data = Dataset::household(NaiveDate::from_ymd_opt(2024, 6, 15).unwrap());
        let connection = |key: &str| {
            data.bank_connections
                .iter()
                .find(|connection| connection.key == key)
                .unwrap()
        };
        let connected: Vec<_> = data
            .bank_accounts
            .iter()
            .filter_map(|account| Some((account, account.connection.as_ref()?)))
            .collect();
        let manual = || data.bank_accounts.iter().filter(|a| a.connection.is_none());

        let expired = |key: &str| connection(key).access_valid_for < TimeDelta::zero();
        assert!(connected.iter().any(|(_, c)| expired(c.connection)));
        assert!(connected.iter().any(|(_, c)| !expired(c.connection)));
        assert!(connected.iter().any(|(_, c)| c.last_synced_ago.is_none()));
        assert!(connected.iter().any(|(a, _)| a.name.is_none()));
        assert!(connected.iter().any(|(a, _)| a.iban.is_none()));
        assert!(manual().any(|a| a.ledger_account.is_some()));
        assert!(manual().any(|a| a.ledger_account.is_none() && a.currency != "EUR"));

        // Connections on the mapping page have no bank accounts of their own yet.
        let [(new, _), (from_manual, _), (empty, _)] = MAPPING_CONNECTIONS;
        for (key, _) in MAPPING_CONNECTIONS {
            assert!(connected.iter().all(|(_, c)| c.connection != key));
        }
        // "Already linked" means the same provider account is linked through
        // another connection.
        let already_linked =
            |uid: &str| connected.iter().any(|(_, c)| c.provider_account_uid == uid);
        let offered = &connection(new).provider_accounts;
        assert!(offered.iter().any(|account| already_linked(&account.uid)));
        assert!(offered.iter().any(|account| !already_linked(&account.uid)));
        assert!(offered.iter().any(|a| a.name.is_none() && a.iban.is_none()));
        assert!(connection(from_manual).source_bank_account.is_some());
        assert!(connection(empty).provider_accounts.is_empty());
    }

    /// The composer only suggests a transfer for two pending transactions on
    /// different bank accounts with the same date and opposite amounts.
    #[test]
    fn today_always_has_a_transfer_to_match() {
        for (today, data) in datasets() {
            let pending: Vec<_> = data
                .bank_transactions
                .iter()
                .filter(|txn| txn.status == BankTransactionStatus::Pending && txn.date == today)
                .collect();

            assert!(
                pending.iter().any(|a| pending
                    .iter()
                    .any(|b| a.bank_account != b.bank_account && a.amount == -b.amount)),
                "{today}"
            );
        }
    }

    #[test]
    fn every_status_and_account_is_used() {
        for (today, data) in datasets() {
            for status in [
                BankTransactionStatus::Pending,
                BankTransactionStatus::Linked,
                BankTransactionStatus::Dismissed,
            ] {
                assert!(
                    data.bank_transaction_count(status) > 0,
                    "{today}: no {status} transactions"
                );
            }
            for account in &data.accounts {
                let used = data
                    .transactions
                    .iter()
                    .flat_map(|txn| &txn.lines)
                    .any(|line| line.account == account.key);
                // One account is left unused so that it can be deleted.
                let key = account.key;
                assert_eq!(used, key != HOME_IMPROVEMENT, "{today}: {key}");
            }
        }
    }

    /// A pending transaction on a day that already has journal entries for the
    /// same account can be linked from the transaction edit form.
    #[test]
    fn some_pending_transactions_fall_on_booked_days() {
        for (today, data) in datasets() {
            for bank_account in [EVERYDAY, VISA] {
                let on_booked_day = |pending: &&BankTxn| {
                    data.bank_transactions.iter().any(|txn| {
                        txn.bank_account == bank_account
                            && txn.status == BankTransactionStatus::Linked
                            && txn.date == pending.date
                    })
                };
                let count = data
                    .bank_transactions
                    .iter()
                    .filter(|txn| txn.bank_account == bank_account)
                    .filter(|txn| txn.status == BankTransactionStatus::Pending)
                    .filter(on_booked_day)
                    .count();
                assert!(count > 0, "{today}: {bank_account}");
            }
        }
    }
}
