-- OxiLedger User Preferences Migration
-- Stores per-user settings, such as how the navigation is arranged.

-- One JSON object with a key per feature, e.g. "navigation".
ALTER TABLE users
    ADD COLUMN preferences JSONB NOT NULL DEFAULT '{}'::jsonb;
