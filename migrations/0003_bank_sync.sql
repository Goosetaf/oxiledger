-- Bank sync: connections, accounts, staged transactions, and auth state table.

-- A bank connection represents a single authorization session with a provider
-- (e.g. one Enable Banking session covering one or more bank accounts).
CREATE TABLE bank_connections (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id              UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_id          VARCHAR(100) NOT NULL,        -- e.g. "enable_banking"
    provider_session_id  VARCHAR(255) NOT NULL,         -- provider-side session UUID
    aspsp_name           VARCHAR(255),                  -- e.g. "Nordea"
    aspsp_country        VARCHAR(10),                   -- e.g. "FI"
    access_valid_until   TIMESTAMPTZ,                   -- when the consent expires
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_bank_connections_user ON bank_connections(user_id);

-- A specific bank account within a connection, linked to an internal ledger account.
CREATE TABLE bank_accounts (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id              UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bank_connection_id   UUID NOT NULL REFERENCES bank_connections(id) ON DELETE CASCADE,
    internal_account_id  UUID NOT NULL REFERENCES accounts(id),
    provider_account_uid VARCHAR(255) NOT NULL,         -- uid from the provider
    iban                 VARCHAR(50),
    name                 VARCHAR(255),
    currency             VARCHAR(10) NOT NULL DEFAULT 'EUR',
    last_synced_at       TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(bank_connection_id, provider_account_uid)
);

CREATE INDEX idx_bank_accounts_user ON bank_accounts(user_id);
CREATE INDEX idx_bank_accounts_internal_account ON bank_accounts(internal_account_id);

-- Imported (staged) bank transactions waiting to be linked to a ledger transaction.
-- amount > 0 means money coming into the account (credit to the bank);
-- amount < 0 means money leaving the account (debit to the bank).
CREATE TABLE bank_transactions (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bank_account_id         UUID NOT NULL REFERENCES bank_accounts(id) ON DELETE CASCADE,
    user_id                 UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    external_id             VARCHAR(255) NOT NULL,      -- provider's transaction ID
    date                    DATE NOT NULL,
    amount                  NUMERIC(19,4) NOT NULL,     -- signed: positive = inflow
    currency                VARCHAR(10) NOT NULL,
    description             TEXT NOT NULL,
    reference               VARCHAR(255),
    status                  VARCHAR(20) NOT NULL DEFAULT 'pending',  -- pending | linked | dismissed
    linked_journal_entry_id UUID,                       -- set once linked, FK added in migration 0004
    imported_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(bank_account_id, external_id)                -- idempotent import
);

CREATE INDEX idx_bank_transactions_user   ON bank_transactions(user_id);
CREATE INDEX idx_bank_transactions_status ON bank_transactions(bank_account_id, status);

-- Temporary state records for in-flight bank authorization flows.
-- Consumed (deleted) by the callback handler after use.
CREATE TABLE bank_auth_states (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_id   VARCHAR(100) NOT NULL,
    state         VARCHAR(255) UNIQUE NOT NULL,         -- CSRF token / random state
    aspsp_name    VARCHAR(255),
    aspsp_country VARCHAR(10),
    expires_at    TIMESTAMPTZ NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_bank_auth_states_state   ON bank_auth_states(state);
CREATE INDEX idx_bank_auth_states_expires ON bank_auth_states(expires_at);
