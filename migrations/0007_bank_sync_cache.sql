-- Cache the last known bank balance directly on the bank account record.

ALTER TABLE bank_accounts
    ADD COLUMN IF NOT EXISTS balance NUMERIC(19,4);
