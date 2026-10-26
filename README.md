# OxiLedger

An application made for tracking personal finances. Built with [Dioxus](https://dioxuslabs.com/) and [Axum](https://docs.rs/axum/).

# Running

This project is primarily designed to run with Docker.

## Configuration

OxiLedger uses environment variables for configuration. They are defined as follows:

| Variable                    | Description                                                                                               | Default                         |
| --------------------------- | --------------------------------------------------------------------------------------------------------- | ------------------------------- |
| `DATABASE_URL`              | The URL for the PostgreSQL database.                                                                      | _required_                      |
| `OIDC_N_ISSUER`             | OIDC issuer URL for authentication.                                                                       | _null_                          |
| `OIDC_N_CLIENT_ID`          | OIDC client ID for authentication.                                                                        | _null_                          |
| `OIDC_N_CLIENT_SECRET`      | OIDC client secret for authentication.                                                                    | _null_                          |
| `OIDC_N_NAME`               | Display name for the OIDC provider.                                                                       | _the issuer URL_                |
| `OIDC_N_REDIRECT_URI`       | Optional. Overrides the redirect URI built from `BASE_URL`.                                               | `<BASE_URL>/auth/oidc/callback` |
| `OIDC_N_ALLOW_REGISTRATION` | If set to `true` or `1`, anyone the provider signs in gets an account on their first sign-in.             | `false`                         |
| `OIDC_N_ALLOWED_GROUPS`     | Comma-separated groups (from the `groups` claim) that can sign in with the provider, or `*` for anyone.   | _required_                      |
| `DISABLE_PASSWORD_LOGIN`    | If set to `true` or `1`, disables password-based login and registration.                                  | `false`                         |
| `ALLOW_REGISTRATION`        | If set to `true` or `1`, anyone can create an account with a username and password.                       | `false`                         |
| `BASE_URL`                  | The base URL for the application, used in OIDC and bank sync redirect URIs. Leave off the trailing slash. | _null_                          |
| `IP`                        | The address the server listens on. The Docker image sets it to `0.0.0.0`.                                 | `127.0.0.1`                     |
| `PORT`                      | The port the server listens on.                                                                           | `8080`                          |

Replace `N` in the OIDC variables with a number (e.g. `OIDC_1_ISSUER`) to configure one or multiple OIDC providers. The numbers must be sequential and start at 1.

> [!NOTE]
> On first start, a setup screen creates the first account, whatever the registration settings are.

### Bank sync

Bank sync imports transactions and balances from users' banks through Enable Banking or GoCardless. Configure one or both, and set `BASE_URL` so banks can send users back to OxiLedger.

#### Enable Banking

Register an application in the [Enable Banking](https://enablebanking.com) control panel and add `<BASE_URL>/bank-sync/callback` as a redirect URL.

| Variable                          | Description                                                       | Default   |
| --------------------------------- | ----------------------------------------------------------------- | --------- |
| `ENABLE_BANKING_APP_ID`           | The application's ID.                                             | _null_    |
| `ENABLE_BANKING_PRIVATE_KEY_PATH` | Path to the application's private key in PEM format.              | _null_    |
| `ENABLE_BANKING_ENVIRONMENT`      | `SANDBOX` or `PRODUCTION`. Use `PRODUCTION` for real deployments. | `SANDBOX` |

#### GoCardless

Uses a secret ID and key from [GoCardless Bank Account Data](https://developer.gocardless.com/bank-account-data).

| Variable                 | Description                                                       | Default   |
| ------------------------ | ----------------------------------------------------------------- | --------- |
| `GOCARDLESS_SECRET_ID`   | The secret ID.                                                    | _null_    |
| `GOCARDLESS_SECRET_KEY`  | The secret key.                                                   | _null_    |
| `GOCARDLESS_ENVIRONMENT` | `SANDBOX` or `PRODUCTION`. Use `PRODUCTION` for real deployments. | `SANDBOX` |

#### Automatic sync

| Variable                | Description                                                                                                                               | Default       |
| ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | ------------- |
| `BANK_SYNC_MODE`        | How connected accounts refresh. `off` (only with the Sync button), `lazy` (when an account is opened) or `background` (on a schedule).    | `off`         |
| `BANK_SYNC_TTL_SECONDS` | How many seconds an account's data may age before it is refreshed. Required for `lazy`. Without it, `background` refreshes every account. | _null_        |
| `BANK_SYNC_SCHEDULE`    | Cron schedule for `background` mode, in UTC.                                                                                              | `0 0 * * * *` |

# Development

To get started, install Rust, dioxus-cli, and sqlx-cli. If you use Nix, the flake provides them.

To start a development database, you can run `docker compose up -d`, which matches the prefilled `.env.example`.

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
