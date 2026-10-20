# OxiLedger

An application made for tracking personal finances. Built with [Dioxus](https://dioxuslabs.com/) and [Axum](https://docs.rs/axum/).

# Running

This project is primarily designed to run with Docker.

## Configuration

OxiLedger uses environment variables for configuration. They are defined as follows:

| Variable                 | Description                                                                                               | Default                         |
| ------------------------ | --------------------------------------------------------------------------------------------------------- | ------------------------------- |
| `DATABASE_URL`           | The URL for the PostgreSQL database.                                                                      | _required_                      |
| `OIDC_N_ISSUER`          | OIDC issuer URL for authentication.                                                                       | _null_                          |
| `OIDC_N_CLIENT_ID`       | OIDC client ID for authentication.                                                                        | _null_                          |
| `OIDC_N_CLIENT_SECRET`   | OIDC client secret for authentication.                                                                    | _null_                          |
| `OIDC_N_NAME`            | Display name for the OIDC provider.                                                                       | _the issuer URL_                |
| `OIDC_N_REDIRECT_URI`    | Redirect URI registered with the OIDC provider. Must point to `/auth/oidc/callback` on this server.       | `<BASE_URL>/auth/oidc/callback` |
| `DISABLE_PASSWORD_LOGIN` | If set to `true` or `1`, disables password-based login and registration.                                  | `false`                         |
| `BASE_URL`               | The base URL for the application, used in OIDC and bank sync redirect URIs. Leave off the trailing slash. | _null_                          |
| `IP`                     | The address the server listens on. The Docker image sets it to `0.0.0.0`.                                 | `127.0.0.1`                     |
| `PORT`                   | The port the server listens on.                                                                           | `8080`                          |

Replace `N` in the OIDC variables with a number (e.g. `OIDC_1_ISSUER`) to configure one or multiple OIDC providers. Numbering starts at 1 and must not skip any, because OxiLedger stops looking at the first missing `OIDC_N_ISSUER`. A provider is skipped if its client ID or secret is missing, or if neither `OIDC_N_REDIRECT_URI` nor `BASE_URL` is set. Providers are loaded when the server starts, so one whose discovery fails stays unavailable until a restart.

The server also reads a `.env` file from its working directory or the nearest parent directory that has one. Variables already set in the environment take precedence.

### Bank sync

Bank sync imports transactions and balances from users' banks through [Enable Banking](https://enablebanking.com) or [GoCardless Bank Account Data](https://developer.gocardless.com/bank-account-data). Each provider appears on the connect page once its credentials are set, and both can be used together. Bank accounts can still be added manually without a provider.

After approving access, users are sent back to `<BASE_URL>/bank-sync/callback`. Enable Banking only redirects to URLs registered on the application, so add this URL there. If `BASE_URL` is unset, bank sync uses `http://localhost:8080`.

| Variable                          | Description                                                                                                               | Default       |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------- | ------------- |
| `ENABLE_BANKING_APP_ID`           | Application ID from the Enable Banking control panel.                                                                     | _null_        |
| `ENABLE_BANKING_PRIVATE_KEY_PATH` | Path to the application's RSA private key in PEM format. In Docker, mount the file and use its path inside the container. | _null_        |
| `ENABLE_BANKING_ENVIRONMENT`      | `SANDBOX` or `PRODUCTION`. Both use the same API address today, so this has no effect.                                    | `SANDBOX`     |
| `GOCARDLESS_SECRET_ID`            | Secret ID for GoCardless Bank Account Data.                                                                               | _null_        |
| `GOCARDLESS_SECRET_KEY`           | Secret key for GoCardless Bank Account Data.                                                                              | _null_        |
| `GOCARDLESS_ENVIRONMENT`          | `SANDBOX` or `PRODUCTION`. Both use the same API address today, so this has no effect.                                    | `SANDBOX`     |
| `BANK_SYNC_MODE`                  | When connected accounts refresh on their own. `off`, `lazy` or `background`.                                              | `off`         |
| `BANK_SYNC_TTL_SECONDS`           | How many seconds an account's data may age before it is refreshed. A positive whole number.                               | _null_        |
| `BANK_SYNC_SCHEDULE`              | Cron schedule for `background` mode, in UTC. Five fields, or six with seconds first.                                      | `0 0 * * * *` |

With `lazy`, an account is refreshed when its detail page is opened and its data is older than `BANK_SYNC_TTL_SECONDS`. Lazy mode never refreshes without a TTL. With `background`, accounts are refreshed on `BANK_SYNC_SCHEDULE`, every hour by default. Only accounts older than the TTL are refreshed, or every account if no TTL is set. The Sync button on an account works in any mode.

In the `.env` file OxiLedger reads, put the schedule in quotes, like `BANK_SYNC_SCHEDULE="0 6 * * *"`. Without quotes, that line and every line after it are silently skipped.

# Development

To get started, install Rust, dioxus-cli, and sqlx-cli. The Nix flake provides all three, and with [direnv](https://direnv.net/) the `.envrc` loads it automatically.

`docker compose up -d` starts a PostgreSQL database that matches the `DATABASE_URL` in `.env.example`.

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
