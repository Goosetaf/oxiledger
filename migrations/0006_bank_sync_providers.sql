-- Support multi-provider bank auth flows and linking an existing manual bank
-- account during provider authorization.

ALTER TABLE bank_auth_states
    ADD COLUMN IF NOT EXISTS provider_reference TEXT;

ALTER TABLE bank_auth_states
    ADD COLUMN IF NOT EXISTS source_bank_account_id UUID REFERENCES bank_accounts(id) ON DELETE SET NULL;

ALTER TABLE bank_connections
    ADD COLUMN IF NOT EXISTS provider_accounts_json JSONB;

ALTER TABLE bank_connections
    ADD COLUMN IF NOT EXISTS source_bank_account_id UUID REFERENCES bank_accounts(id) ON DELETE SET NULL;
