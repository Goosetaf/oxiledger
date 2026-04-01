-- OxiLedger OIDC Migration
-- Enforces uniqueness per provider identity and adds a short-lived state table
-- for the PKCE / CSRF token used during the OAuth 2.0 authorization code flow.

-- Ensure that a (provider, subject) pair maps to exactly one user.
-- A UNIQUE CONSTRAINT (not just an index) is required for ON CONFLICT to work.
ALTER TABLE users
    ADD CONSTRAINT uq_users_oidc UNIQUE (oidc_provider, oidc_subject);

-- Short-lived PKCE + CSRF state rows created at the start of each OIDC flow.
-- They are consumed (deleted) on callback and automatically expire after 10 min.
CREATE TABLE oidc_states (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider_id VARCHAR(100)  NOT NULL,   -- e.g. "1", "2" (matches OIDC_N_ env prefix)
    csrf_token  VARCHAR(255)  NOT NULL,   -- random opaque value sent as `state` param
    pkce_verifier VARCHAR(255) NOT NULL,  -- PKCE code_verifier (plain, never sent to browser)
    nonce       VARCHAR(255)  NOT NULL,   -- OIDC nonce claim value
    expires_at  TIMESTAMPTZ   NOT NULL,
    created_at  TIMESTAMPTZ   NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX idx_oidc_states_csrf ON oidc_states (csrf_token);
CREATE INDEX idx_oidc_states_expires ON oidc_states (expires_at);
