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

To get started, install Rust, dioxus-cli, and sqlx-cli.

## Test data

`cargo seed` applies the migrations and fills the database with test data.

```bash
cp .env.example .env
docker compose up -d --wait
cargo seed
```

Log in as `dev` with the password `dev-password`. This user has about six months of accounts, transactions and bank accounts in the states the app can show. The user `dev-empty` has the same password and no data.

Run `cargo seed` again to reset these users. Only run it against a development database.

Connecting a bank, syncing and signing in through OIDC need real provider credentials and are not covered.

The seeder is a separate program in `seed/` and is not part of the app.

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

When building the Docker image, SQLx needs to run in offline mode. This requires prepared files, which can be generated with the following command:

```bash
cargo sqlx prepare --workspace -- --features server
```

# Contributing

This is a personal hobby project, but contributions are welcome! If you have an idea for a feature or improvement, feel free to open an issue or submit a pull request.

## AI Use

Using AI to generate code is permitted, but you **MUST** review and understand any AI-generated code before submitting it. Any PRs containing AI-generated code that has not been human-reviewed will be rejected.
