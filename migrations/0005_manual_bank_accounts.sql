-- Allow bank_accounts to exist without an external provider connection.
-- A manual account has no bank_connection_id, provider_account_uid, or sync support.

-- Make bank_connection_id optional.
ALTER TABLE bank_accounts
    ALTER COLUMN bank_connection_id DROP NOT NULL;

-- Make provider_account_uid optional (manual accounts have none).
ALTER TABLE bank_accounts
    ALTER COLUMN provider_account_uid DROP NOT NULL;

-- Drop the unique constraint that requires both columns; replace with a
-- partial unique index that only applies to provider-backed accounts.
ALTER TABLE bank_accounts
    DROP CONSTRAINT bank_accounts_bank_connection_id_provider_account_uid_key;

CREATE UNIQUE INDEX idx_bank_accounts_provider_uid
    ON bank_accounts(bank_connection_id, provider_account_uid)
    WHERE bank_connection_id IS NOT NULL AND provider_account_uid IS NOT NULL;

-- Add a flag so the UI and sync logic can distinguish manual from synced accounts.
ALTER TABLE bank_accounts
    ADD COLUMN IF NOT EXISTS is_manual BOOLEAN NOT NULL DEFAULT FALSE;
