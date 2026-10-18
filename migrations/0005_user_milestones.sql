-- OxiLedger User Milestones Migration
-- Records things a user has done that the rest of their data can't show, such as skipping setup.

CREATE TABLE user_milestones (
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key        VARCHAR(100) NOT NULL,
    reached_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, key)
);
