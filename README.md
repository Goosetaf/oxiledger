# OxiLedger

An application made for tracking personal finances. Built with [Dioxus](https://dioxuslabs.com/) and [Axum](https://docs.rs/axum/).

# Running

This project is primarily designed to run with Docker.

## Configuration

OxiLedger uses environment variables for configuration. They are defined as follows:

| Variable                    | Description                                                                                               | Default                         |
| --------------------------- | --------------------------------------------------------------------------------------------------------- | ------------------------------- |
| `DATABASE_URL`              | Where the data is stored: a SQLite file or a PostgreSQL database. See [Database](#database).              | `sqlite://data/oxiledger.db`    |
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

### Database

OxiLedger stores its data in SQLite by default, in the file `data/oxiledger.db` under the working directory, which is `/app` in the Docker image. Mount a volume at `/app/data`, for example with `-v oxiledger-data:/app/data`, to keep the data when the container is replaced.

Set `DATABASE_URL` to use another SQLite file or a PostgreSQL database:

| Database   | `DATABASE_URL`                             |
| ---------- | ------------------------------------------ |
| SQLite     | `sqlite://path/to/oxiledger.db`            |
| PostgreSQL | `postgres://user:password@host/database`   |

For an absolute SQLite path, write three slashes: `sqlite:///var/lib/oxiledger/oxiledger.db`.

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

To get started, install Rust and dioxus-cli. If you use Nix, the flake provides them.

Copy `.env.example` to `.env`. It stores data in a SQLite file under `data/`. To develop against PostgreSQL instead, run `docker compose up -d` and switch `DATABASE_URL` to the PostgreSQL line in `.env`.

## Serving

Run the following command in the root of your project to start developing with the default platform:

```bash
dx serve --platform web
```

To run for a different platform, use the `--platform platform` flag. E.g.

```bash
dx serve --platform desktop
```

## Database layer

The application reads and writes data through the repository traits in `src/storage`, one per kind of data. `src/storage/diesel_store` implements them with [Diesel](https://diesel.rs/) for both SQLite and PostgreSQL, and is the only code that may use Diesel.

Each database has its own migrations, in `migrations/sqlite` and `migrations/postgres`. They are built into the server and run when it starts. A schema change needs a migration in both folders, with the same name, and a matching change to `src/storage/diesel_store/schema.rs`.

## Testing

```bash
cargo test --no-default-features --features server
```

The storage tests run against SQLite. To run them against PostgreSQL as well, point `OXILEDGER_TEST_POSTGRES_URL` at a database, such as the one from `docker compose up -d`:

```bash
OXILEDGER_TEST_POSTGRES_URL=postgres://postgres:password@localhost/oxiledger cargo test --no-default-features --features server
```


# Contributing

This is a personal hobby project, but contributions are welcome! If you have an idea for a feature or improvement, feel free to open an issue or submit a pull request.

## AI Use

Using AI to generate code is permitted, but you **MUST** review and understand any AI-generated code before submitting it. Any PRs containing AI-generated code that has not been human-reviewed will be rejected.
