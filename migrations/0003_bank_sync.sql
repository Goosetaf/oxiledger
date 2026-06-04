-- Bank sync: connections, accounts, staged transactions, auth state, and balance cache.

-- A bank connection represents a single authorization session with a provider
-- (e.g. one Enable Banking session covering one or more bank accounts).
CREATE TABLE bank_connections (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_id            VARCHAR(100) NOT NULL,       -- e.g. "enable_banking"
    provider_session_id    VARCHAR(255) NOT NULL,        -- provider-side session UUID
    aspsp_name             VARCHAR(255),                 -- e.g. "Nordea"
    aspsp_country          VARCHAR(10),                  -- e.g. "FI"
    access_valid_until     TIMESTAMPTZ,                  -- when the consent expires
    provider_accounts_json JSONB,                        -- raw account list from provider
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
    -- source_bank_account_id added below after bank_accounts is created
);

CREATE INDEX idx_bank_connections_user ON bank_connections(user_id);

-- A specific bank account within a connection, linked to an internal ledger account.
-- bank_connection_id and provider_account_uid are NULL for manual (non-synced) accounts.
CREATE TABLE bank_accounts (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id              UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bank_connection_id   UUID REFERENCES bank_connections(id) ON DELETE CASCADE,
    internal_account_id  UUID NOT NULL REFERENCES accounts(id),
    provider_account_uid VARCHAR(255),                   -- uid from the provider; NULL for manual
    iban                 VARCHAR(50),
    name                 VARCHAR(255),
    currency             VARCHAR(10) NOT NULL DEFAULT 'EUR',
    is_manual            BOOLEAN NOT NULL DEFAULT FALSE,
    balance              NUMERIC(19,4),                  -- last known bank balance from sync
    last_synced_at       TIMESTAMPTZ,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_bank_accounts_user             ON bank_accounts(user_id);
CREATE INDEX idx_bank_accounts_internal_account ON bank_accounts(internal_account_id);

-- Provider uniqueness only applies to connected (non-manual) accounts.
CREATE UNIQUE INDEX idx_bank_accounts_provider_uid
    ON bank_accounts(bank_connection_id, provider_account_uid)
    WHERE bank_connection_id IS NOT NULL AND provider_account_uid IS NOT NULL;

-- Back-link from bank_connections to the manual account used as the source during setup.
-- Added after bank_accounts to resolve the circular reference.
ALTER TABLE bank_connections
    ADD COLUMN source_bank_account_id UUID REFERENCES bank_accounts(id) ON DELETE SET NULL;

-- Imported (staged) bank transactions waiting to be linked to a ledger transaction.
-- amount > 0 means money coming into the account (inflow);
-- amount < 0 means money leaving the account (outflow).
CREATE TABLE bank_transactions (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    bank_account_id         UUID NOT NULL REFERENCES bank_accounts(id) ON DELETE CASCADE,
    user_id                 UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    external_id             VARCHAR(255) NOT NULL,       -- provider's transaction ID
    date                    DATE NOT NULL,
    amount                  NUMERIC(19,4) NOT NULL,      -- signed: positive = inflow
    currency                VARCHAR(10) NOT NULL,
    description             TEXT NOT NULL,
    reference               VARCHAR(255),
    status                  VARCHAR(20) NOT NULL DEFAULT 'pending', -- pending | linked | dismissed
    linked_journal_entry_id UUID,                        -- FK added below after journal_entries column exists
    imported_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(bank_account_id, external_id)                 -- idempotent import
);

CREATE INDEX idx_bank_transactions_user   ON bank_transactions(user_id);
CREATE INDEX idx_bank_transactions_status ON bank_transactions(bank_account_id, status);

-- Back-link from a journal entry to the bank transaction that originated it.
-- ON DELETE SET NULL: deleting a bank transaction record unlinks it from the entry.
ALTER TABLE journal_entries
    ADD COLUMN bank_transaction_id UUID
        REFERENCES bank_transactions(id)
        ON DELETE SET NULL;

-- Now that journal_entries.bank_transaction_id exists, add the FK from
-- bank_transactions.linked_journal_entry_id (avoids a circular dependency at table-creation time).
ALTER TABLE bank_transactions
    ADD CONSTRAINT fk_bank_transactions_linked_entry
        FOREIGN KEY (linked_journal_entry_id)
        REFERENCES journal_entries(id)
        ON DELETE SET NULL;

CREATE INDEX idx_journal_entries_bank_txn
    ON journal_entries(bank_transaction_id)
    WHERE bank_transaction_id IS NOT NULL;

CREATE INDEX idx_bank_transactions_linked_entry
    ON bank_transactions(linked_journal_entry_id)
    WHERE linked_journal_entry_id IS NOT NULL;

-- Temporary state records for in-flight bank authorization flows.
-- Consumed (deleted) by the callback handler after use.
CREATE TABLE bank_auth_states (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider_id            VARCHAR(100) NOT NULL,
    state                  VARCHAR(255) UNIQUE NOT NULL, -- CSRF token / random state
    aspsp_name             VARCHAR(255),
    aspsp_country          VARCHAR(10),
    provider_reference     TEXT,                         -- opaque reference from provider auth response
    source_bank_account_id UUID REFERENCES bank_accounts(id) ON DELETE SET NULL,
    expires_at             TIMESTAMPTZ NOT NULL,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_bank_auth_states_state   ON bank_auth_states(state);
CREATE INDEX idx_bank_auth_states_expires ON bank_auth_states(expires_at);
