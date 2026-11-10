# Oxiledger

Oxiledger is a Rust-based personal economy app. The goal is to have a view of your entire economy, from day-to-day to investments and wealth. It's written as a fullstack Dioxus app (version in cargo.toml).

## Guidlines for writing code

If you are unsure of how to write this version of Dioxus, look up the docs to see what it looks like for the version currently used.

Default to no comments. Only write comments if the behaviour cannot be understood from reading the code itself, or the behaviour is surprising in some way.

## Storage

Data access goes through the repository traits in `src/storage`. Only `src/storage/diesel_store` may use Diesel; server functions, handlers and models must not. Both SQLite and Postgres are supported, so a schema change needs a migration in `migrations/sqlite` and in `migrations/postgres`, and the storage tests should pass against both (see the README).
