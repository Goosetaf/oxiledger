-- Add a back-link from a journal entry to the bank transaction that originated it.
-- ON DELETE SET NULL: deleting a bank transaction record unlinks it from the entry
-- (the entry remains but is no longer treated as bank-imported).
ALTER TABLE journal_entries
    ADD COLUMN bank_transaction_id UUID
        REFERENCES bank_transactions(id)
        ON DELETE SET NULL;

-- Now we can safely add the FK from bank_transactions.linked_journal_entry_id
-- to journal_entries (the column was created in 0003 without a FK to avoid a
-- circular dependency at migration time).
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
