-- IDs are hyphenated UUID text and timestamps are UTC text with microseconds.
-- Money is an integer count of ten thousandths, so 12.34 is 123400.

CREATE TABLE users (
    id            TEXT PRIMARY KEY NOT NULL,
    username      TEXT NOT NULL UNIQUE,
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    oidc_provider TEXT,
    oidc_subject  TEXT,
    preferences   TEXT NOT NULL DEFAULT '{}',
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    UNIQUE (oidc_provider, oidc_subject)
) STRICT;

CREATE TABLE sessions (
    id         TEXT PRIMARY KEY NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token      TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX idx_sessions_user ON sessions(user_id);

CREATE TABLE oidc_states (
    id            TEXT PRIMARY KEY NOT NULL,
    provider_id   TEXT NOT NULL,
    csrf_token    TEXT NOT NULL UNIQUE,
    pkce_verifier TEXT NOT NULL,
    nonce         TEXT NOT NULL,
    expires_at    TEXT NOT NULL,
    created_at    TEXT NOT NULL
) STRICT;

CREATE INDEX idx_oidc_states_expires ON oidc_states(expires_at);

CREATE TABLE accounts (
    id             TEXT PRIMARY KEY NOT NULL,
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    code           TEXT,
    account_type   TEXT NOT NULL
        CHECK (account_type IN ('Asset', 'Liability', 'Equity', 'Revenue', 'Expense')),
    normal_balance TEXT NOT NULL CHECK (normal_balance IN ('Debit', 'Credit')),
    description    TEXT,
    is_active      INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    UNIQUE (user_id, code)
) STRICT;

CREATE INDEX idx_accounts_user ON accounts(user_id);

CREATE TABLE transactions (
    id          TEXT PRIMARY KEY NOT NULL,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    date        TEXT NOT NULL,
    description TEXT NOT NULL,
    reference   TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
) STRICT;

CREATE INDEX idx_transactions_user_date ON transactions(user_id, date);

-- The application checks that the debits and credits of a transaction balance.
CREATE TABLE journal_entries (
    id                  TEXT PRIMARY KEY NOT NULL,
    transaction_id      TEXT NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    account_id          TEXT NOT NULL REFERENCES accounts(id),
    entry_type          TEXT NOT NULL CHECK (entry_type IN ('Debit', 'Credit')),
    amount              INTEGER NOT NULL CHECK (amount > 0),
    memo                TEXT,
    bank_transaction_id TEXT REFERENCES bank_transactions(id) ON DELETE SET NULL,
    created_at          TEXT NOT NULL
) STRICT;

CREATE INDEX idx_journal_entries_transaction ON journal_entries(transaction_id);
CREATE INDEX idx_journal_entries_account ON journal_entries(account_id);
CREATE INDEX idx_journal_entries_bank_txn
    ON journal_entries(bank_transaction_id)
    WHERE bank_transaction_id IS NOT NULL;

CREATE TABLE bank_connections (
    id                     TEXT PRIMARY KEY NOT NULL,
    user_id                TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_id            TEXT NOT NULL,
    provider_session_id    TEXT NOT NULL,
    aspsp_name             TEXT,
    aspsp_country          TEXT,
    access_valid_until     TEXT,
    provider_accounts_json TEXT,
    source_bank_account_id TEXT REFERENCES bank_accounts(id) ON DELETE SET NULL,
    created_at             TEXT NOT NULL
) STRICT;

CREATE INDEX idx_bank_connections_user ON bank_connections(user_id);

CREATE TABLE bank_accounts (
    id                   TEXT PRIMARY KEY NOT NULL,
    user_id              TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bank_connection_id   TEXT REFERENCES bank_connections(id) ON DELETE CASCADE,
    internal_account_id  TEXT REFERENCES accounts(id),
    provider_account_uid TEXT,
    iban                 TEXT,
    name                 TEXT,
    currency             TEXT NOT NULL DEFAULT 'EUR',
    is_manual            INTEGER NOT NULL DEFAULT 0 CHECK (is_manual IN (0, 1)),
    balance              INTEGER,
    last_synced_at       TEXT,
    created_at           TEXT NOT NULL
) STRICT;

CREATE INDEX idx_bank_accounts_user ON bank_accounts(user_id);
CREATE INDEX idx_bank_accounts_internal_account
    ON bank_accounts(internal_account_id)
    WHERE internal_account_id IS NOT NULL;

CREATE UNIQUE INDEX idx_bank_accounts_provider_uid
    ON bank_accounts(bank_connection_id, provider_account_uid)
    WHERE bank_connection_id IS NOT NULL AND provider_account_uid IS NOT NULL;

-- A positive amount is money coming into the account.
-- The status is pending, linked or dismissed.
CREATE TABLE bank_transactions (
    id                      TEXT PRIMARY KEY NOT NULL,
    bank_account_id         TEXT NOT NULL REFERENCES bank_accounts(id) ON DELETE CASCADE,
    user_id                 TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    external_id             TEXT NOT NULL,
    date                    TEXT NOT NULL,
    amount                  INTEGER NOT NULL,
    currency                TEXT NOT NULL,
    description             TEXT NOT NULL,
    reference               TEXT,
    status                  TEXT NOT NULL DEFAULT 'pending',
    linked_journal_entry_id TEXT REFERENCES journal_entries(id) ON DELETE SET NULL,
    imported_at             TEXT NOT NULL,
    UNIQUE (bank_account_id, external_id)
) STRICT;

CREATE INDEX idx_bank_transactions_user ON bank_transactions(user_id);
CREATE INDEX idx_bank_transactions_status ON bank_transactions(bank_account_id, status);
CREATE INDEX idx_bank_transactions_linked_entry
    ON bank_transactions(linked_journal_entry_id)
    WHERE linked_journal_entry_id IS NOT NULL;

CREATE TABLE bank_auth_states (
    id                     TEXT PRIMARY KEY NOT NULL,
    user_id                TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_id            TEXT NOT NULL,
    state                  TEXT NOT NULL UNIQUE,
    aspsp_name             TEXT,
    aspsp_country          TEXT,
    provider_reference     TEXT,
    source_bank_account_id TEXT REFERENCES bank_accounts(id) ON DELETE SET NULL,
    expires_at             TEXT NOT NULL,
    created_at             TEXT NOT NULL
) STRICT;

CREATE INDEX idx_bank_auth_states_expires ON bank_auth_states(expires_at);
