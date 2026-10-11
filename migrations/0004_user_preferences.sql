-- Per-user preferences: how each person has tailored the app to their own economy.
--
-- One JSON object with a key per feature (currently only "navigation"), so a
-- new preference never needs a migration. Each feature owns the shape of its
-- key and falls back to its defaults when the key is missing or unreadable.
ALTER TABLE users
    ADD COLUMN preferences JSONB NOT NULL DEFAULT '{}'::jsonb;
