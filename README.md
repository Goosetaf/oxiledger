# OxiLedger

An application made for tracking personal finances. Built with [Dioxus](https://dioxuslabs.com/) and [Axum](https://docs.rs/axum/).

# Running

This project is primarily designed to run with Docker.

## Configuration

OxiLedger uses environment variables for configuration. They are defined as follows:

| Variable                 | Description                                                                 | Default    |
| ------------------------ | --------------------------------------------------------------------------- | ---------- |
| `DATABASE_URL`           | The URL for the PostgreSQL database.                                        | _required_ |
| `OIDC_N_ISSUER`          | OIDC issuer URL for authentication.                                         | _null_     |
| `OIDC_N_CLIENT_ID`       | OIDC client ID for authentication.                                          | _null_     |
| `OIDC_N_CLIENT_SECRET`   | OIDC client secret for authentication.                                      | _null_     |
| `OIDC_N_NAME`            | Display name for the OIDC provider.                                         | _null_     |
| `DISABLE_PASSWORD_LOGIN` | If set to `true`, disables password-based login.                            | `false`    |
| `BASE_URL`               | The base URL for the application, used in OIDC and bank sync redirect URIs. | _null_     |

Replace `N` in the OIDC variables with a number (e.g. `OIDC_1_ISSUER`) to configure one or multiple OIDC providers.

### Bank sync

Bank sync imports transactions from users' banks. It is available once at least one provider is configured, and `BASE_URL` must be the address users reach OxiLedger at, since banks send them back there.

| Variable                          | Description                                                                                                                                           | Default     |
| --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ----------- |
| `ENABLE_BANKING_APP_ID`           | Application ID for [Enable Banking](https://enablebanking.com).                                                                                       | _null_      |
| `ENABLE_BANKING_PRIVATE_KEY_PATH` | Path to the application's private key. In Docker, a path inside the container.                                                                        | _null_      |
| `ENABLE_BANKING_ENVIRONMENT`      | `SANDBOX` or `PRODUCTION`.                                                                                                                            | `SANDBOX`   |
| `GOCARDLESS_SECRET_ID`            | Secret ID for [GoCardless Bank Account Data](https://developer.gocardless.com/bank-account-data).                                                     | _null_      |
| `GOCARDLESS_SECRET_KEY`           | Secret key for GoCardless Bank Account Data.                                                                                                          | _null_      |
| `GOCARDLESS_ENVIRONMENT`          | `SANDBOX` or `PRODUCTION`.                                                                                                                            | `SANDBOX`   |
| `BANK_SYNC_MODE`                  | When connected accounts refresh. `off`, `lazy` (when an account's page is opened and its data is older than the TTL) or `background` (on a schedule). | `off`       |
| `BANK_SYNC_TTL_SECONDS`           | How old an account's data may get before it is refreshed. Without it, `background` refreshes every account on each run.                               | _null_      |
| `BANK_SYNC_SCHEDULE`              | Cron schedule for `background` mode.                                                                                                                  | `0 * * * *` |

# Development

To get started, install Rust, dioxus-cli, and sqlx-cli.

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
