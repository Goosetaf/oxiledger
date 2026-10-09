# OxiLedger

An application made for tracking personal finances. Built with [Dioxus](https://dioxuslabs.com/) and [Axum](https://docs.rs/axum/).

# Running

This project is primarily designed to run with Docker.

## Configuration

OxiLedger uses environment variables for configuration. They are defined as follows:

| Variable                 | Description                                                   | Default    |
| ------------------------ | ------------------------------------------------------------- | ---------- |
| `DATABASE_URL`           | The URL for the PostgreSQL database.                          | _required_ |
| `OIDC_N_ISSUER`          | OIDC issuer URL for authentication.                           | _null_     |
| `OIDC_N_CLIENT_ID`       | OIDC client ID for authentication.                            | _null_     |
| `OIDC_N_CLIENT_SECRET`   | OIDC client secret for authentication.                        | _null_     |
| `OIDC_N_NAME`            | Display name for the OIDC provider.                           | _null_     |
| `DISABLE_PASSWORD_LOGIN` | If set to `true`, disables password-based login.              | `false`    |
| `BASE_URL`               | The base URL for the application, used in OIDC redirect URIs. | _null_     |

Replace `N` in the OIDC variables with a number (e.g. `OIDC_1_ISSUER`) to configure one or multiple OIDC providers.

# Development

To get started, install Rust, dioxus-cli, and sqlx-cli. With Nix, `nix develop` (or direnv) provides all of them.

## Getting started

```bash
cp .env.example .env
docker compose up -d --wait   # PostgreSQL
cargo seed                    # migrations and test data
dx serve --platform web
```

Then open http://localhost:8080 and log in as `dev` with the password `dev-password`.

The first `cargo seed` and the first `dx serve` each compile the project, which takes a few minutes.

Every checkout needs a database of its own. If port 5432 is already taken, for example by another checkout of this project, set `DB_PORT` in `.env` and use the same port in `DATABASE_URL` before you start the database.

## Seed data

`cargo seed` applies the migrations and fills the database with realistic test data, so there is something to develop and test against on every page that does not depend on an external provider. It creates three users:

| Username    | Password       | Contents                                                   |
| ----------- | -------------- | ---------------------------------------------------------- |
| `dev`       | `dev-password` | A household's finances, described below.                   |
| `dev-empty` | `dev-password` | Nothing, for working on empty states.                      |
| `dev-sso`   | _none_         | An OIDC-only user. Signing in with a password is rejected. |

The `dev` user has about six months of history ending today:

- A chart of accounts with every account type, including inactive accounts, an account without a code, and an unused account that can be deleted.
- Journal transactions: bank transactions booked against an expense, splits such as the monthly salary, transfers between own accounts, and manual entries.
- Bank accounts in each state: connected with an active consent (Nordea), connected with an expired one (Bank Norwegian), connected but never synced, and manual with and without a ledger account, one of them in SEK.
- Bank transactions that are linked, pending, and dismissed, including one without a description. Today's pending ones include a matching pair that the transaction composer suggests as a transfer.
- Balance comparisons: Nordea Savings is balanced, Nordea Everyday differs by its pending transactions, the Bank Norwegian card has a difference that its transactions do not explain, and the manual accounts have no bank balance.

Dates are relative to the day you seed. Run `cargo seed` again to reset these users, or to bring the dates up to today. Accounts, bank accounts and connections keep their ids, so their URLs still work, but you have to log in again.

The seeder only replaces users it created, and stops if someone else already has one of these usernames. It creates users with a known password, so only run it against a development database.

Connecting a bank, syncing, and signing in through OIDC need real provider credentials and are not covered. Syncing a seeded account fails for that reason, so leave `BANK_SYNC_MODE` unset. The account-mapping page normally only opens after a provider callback, so three connections are left waiting on it instead:

- `/bank-accounts/connect/map?connection_id=5eed0000-0000-0000-c84a-292dd8abaa46` is a new connection with one account that is already linked.
- `/bank-accounts/connect/map?connection_id=5eed0000-0000-0000-a5eb-0449632b6e84` was started from a manual bank account.
- `/bank-accounts/connect/map?connection_id=5eed0000-0000-0000-419e-ab9186cd0a44` is a connection for which the bank returned no accounts.

The seeder is behind the `seed` Cargo feature and is not part of the Docker image. The data is defined in `src/server/seed/dataset.rs`.

## Serving

Run the following command in the root of your project to start developing with the default platform:

```bash
dx serve --platform web
```

To run for a different platform, use the `--platform platform` flag. E.g.

```bash
dx serve --platform desktop
```

## Preparing Database Files

SQLx checks queries at compile time. With `SQLX_OFFLINE=true`, as in `.env.example` and the Docker build, it checks them against the prepared files in `.sqlx` instead of a database. Regenerate those files whenever a query or migration changes, with the database running:

```bash
sqlx migrate run
cargo sqlx prepare --workspace -- --features server,seed
```

# Contributing

This is a personal hobby project, but contributions are welcome! If you have an idea for a feature or improvement, feel free to open an issue or submit a pull request.

## AI Use

Using AI to generate code is permitted, but you **MUST** review and understand any AI-generated code before submitting it. Any PRs containing AI-generated code that has not been human-reviewed will be rejected.
