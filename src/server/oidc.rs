//! OIDC / OAuth 2.0 authorization code flow with PKCE.
//!
//! # Configuration
//!
//! Providers are configured via numbered environment variables.  Each set of
//! vars shares the prefix `OIDC_N_` where `N` is a positive integer starting
//! at `1`.  There is no upper limit on the number of providers; the loader
//! stops at the first gap.
//!
//! | Variable            | Required | Description                                          |
//! |---------------------|----------|------------------------------------------------------|
//! | `OIDC_N_ISSUER`     | yes      | OIDC issuer URL (used for discovery)                 |
//! | `OIDC_N_CLIENT_ID`  | yes      | OAuth 2.0 client ID                                  |
//! | `OIDC_N_CLIENT_SECRET` | yes   | OAuth 2.0 client secret                              |
//! | `OIDC_N_NAME`       | no       | Display name shown on login buttons (default: issuer)|
//! | `OIDC_N_REDIRECT_URI` | no     | Override redirect URI (default: auto-derived from `BASE_URL`) |
//!
//! `BASE_URL` must be set when auto-deriving redirect URIs, e.g.
//! `BASE_URL=https://ledger.example.com`.
//!
//! Classic username+password login can be disabled site-wide with:
//! ```
//! DISABLE_PASSWORD_LOGIN=true
//! ```

use openidconnect::{
    core::{CoreClient, CoreProviderMetadata, CoreResponseType},
    reqwest::async_http_client,
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope, TokenResponse,
};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

// ── Config types ─────────────────────────────────────────────────────────────

/// A single fully-configured OIDC provider (secrets included — server only).
#[derive(Clone)]
pub struct OidcProvider {
    /// Stable string ID derived from the env-var index, e.g. `"1"`, `"2"`.
    pub id: String,
    /// Display name shown on the login button.
    pub name: String,
    /// Discovered + configured openidconnect client.
    pub client: CoreClient,
    /// Redirect URI registered with the provider.
    pub redirect_uri: String,
}

/// App-level OIDC configuration shared across requests via `Arc`.
#[derive(Clone, Default)]
pub struct OidcConfig {
    pub providers: Vec<OidcProvider>,
    /// When `true`, the `/api/auth/login` endpoint and the login form are disabled.
    pub disable_password_login: bool,
}

impl OidcConfig {
    /// Returns a minimal [`OidcProviderInfo`] list safe to send to the client.
    pub fn provider_infos(&self) -> Vec<crate::models::oidc::OidcProviderInfo> {
        self.providers
            .iter()
            .map(|p| crate::models::oidc::OidcProviderInfo {
                id: p.id.clone(),
                name: p.name.clone(),
            })
            .collect()
    }

    /// Find a provider by its ID string.
    pub fn find(&self, id: &str) -> Option<&OidcProvider> {
        self.providers.iter().find(|p| p.id == id)
    }
}

// ── Config loading ────────────────────────────────────────────────────────────

/// Load all OIDC providers from environment variables and perform OIDC discovery
/// for each.  Providers without a complete set of required vars are skipped with
/// a warning; a gap in numbering stops the scan.
pub async fn load_config() -> OidcConfig {
    let disable_password_login = std::env::var("DISABLE_PASSWORD_LOGIN")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    let base_url = std::env::var("BASE_URL").unwrap_or_default();

    let mut providers = Vec::new();

    for n in 1u32.. {
        let prefix = format!("OIDC_{}_", n);

        let issuer = match std::env::var(format!("{prefix}ISSUER")) {
            Ok(v) => v,
            Err(_) => break, // gap → stop scanning
        };

        let client_id = match std::env::var(format!("{prefix}CLIENT_ID")) {
            Ok(v) => v,
            Err(_) => {
                eprintln!("[oidc] OIDC_{n}_CLIENT_ID missing — skipping provider {n}");
                continue;
            }
        };

        let client_secret = match std::env::var(format!("{prefix}CLIENT_SECRET")) {
            Ok(v) => v,
            Err(_) => {
                eprintln!("[oidc] OIDC_{n}_CLIENT_SECRET missing — skipping provider {n}");
                continue;
            }
        };

        let name = std::env::var(format!("{prefix}NAME")).unwrap_or_else(|_| issuer.clone());

        let redirect_uri = std::env::var(format!("{prefix}REDIRECT_URI"))
            .unwrap_or_else(|_| format!("{base_url}/auth/oidc/callback"));

        // Perform OIDC discovery (fetches /.well-known/openid-configuration).
        let issuer_url = match IssuerUrl::new(issuer.clone()) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("[oidc] Invalid issuer URL for provider {n}: {e}");
                continue;
            }
        };

        let metadata =
            match CoreProviderMetadata::discover_async(issuer_url, async_http_client).await {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("[oidc] Discovery failed for provider {n} ({issuer}): {e}");
                    continue;
                }
            };

        let client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(client_id),
            Some(ClientSecret::new(client_secret)),
        )
        .set_redirect_uri(match RedirectUrl::new(redirect_uri.clone()) {
            Ok(u) => u,
            Err(e) => {
                eprintln!("[oidc] Invalid redirect URI for provider {n}: {e}");
                continue;
            }
        });

        println!("[oidc] Registered provider {n}: {name}");
        providers.push(OidcProvider {
            id: n.to_string(),
            name,
            client,
            redirect_uri,
        });
    }

    OidcConfig {
        providers,
        disable_password_login,
    }
}

// ── Axum handlers ─────────────────────────────────────────────────────────────

/// `GET /auth/oidc/:provider_id/start`
///
/// Generates a PKCE challenge + CSRF state, persists them in `oidc_states`,
/// then redirects the browser to the provider's authorization endpoint.
pub async fn handle_start(
    axum::extract::Path(provider_id): axum::extract::Path<String>,
    axum::extract::Extension(pool): axum::extract::Extension<PgPool>,
    axum::extract::Extension(config): axum::extract::Extension<Arc<OidcConfig>>,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    let provider = match config.find(&provider_id) {
        Some(p) => p,
        None => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                format!("Unknown OIDC provider: {provider_id}"),
            )
                .into_response();
        }
    };

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let nonce = Nonce::new_random();
    let nonce_for_url = nonce.clone();

    let (auth_url, csrf_token, _returned_nonce) = provider
        .client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            move || nonce_for_url,
        )
        .add_scope(Scope::new("openid".to_string()))
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);

    let result = sqlx::query!(
        "INSERT INTO oidc_states (provider_id, csrf_token, pkce_verifier, nonce, expires_at) \
         VALUES ($1, $2, $3, $4, $5)",
        provider_id,
        csrf_token.secret(),
        pkce_verifier.secret(),
        nonce.secret(),
        expires_at
    )
    .execute(&pool)
    .await;

    if let Err(e) = result {
        eprintln!("[oidc] Failed to store state for provider {provider_id}: {e}");
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to initiate OIDC flow",
        )
            .into_response();
    }

    axum::response::Redirect::temporary(auth_url.as_str()).into_response()
}

/// `GET /auth/oidc/callback?code=...&state=...`
///
/// Validates the CSRF state, exchanges the authorization code for tokens, extracts
/// identity claims, upserts the user row, creates a session, and redirects to `/`.
pub async fn handle_callback(
    axum::extract::Query(params): axum::extract::Query<CallbackParams>,
    axum::extract::Extension(pool): axum::extract::Extension<PgPool>,
    axum::extract::Extension(config): axum::extract::Extension<Arc<OidcConfig>>,
    axum::extract::Extension(cookies): axum::extract::Extension<tower_cookies::Cookies>,
) -> axum::response::Response {
    use axum::response::IntoResponse;

    // 1. Look up + consume the state row (validates CSRF + retrieves PKCE verifier).
    let state_row = sqlx::query!(
        "DELETE FROM oidc_states \
         WHERE csrf_token = $1 AND expires_at > NOW() \
         RETURNING provider_id, pkce_verifier, nonce",
        params.state
    )
    .fetch_optional(&pool)
    .await;

    let state_row = match state_row {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                "Invalid or expired OIDC state. Please try signing in again.",
            )
                .into_response();
        }
        Err(e) => {
            eprintln!("[oidc] DB error fetching state: {e}");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Authentication error",
            )
                .into_response();
        }
    };

    let provider = match config.find(&state_row.provider_id) {
        Some(p) => p,
        None => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                "Provider no longer configured",
            )
                .into_response();
        }
    };

    // 2. Exchange authorization code for tokens.
    let code = AuthorizationCode::new(params.code);
    let pkce_verifier = PkceCodeVerifier::new(state_row.pkce_verifier);
    let nonce = Nonce::new(state_row.nonce);

    let token_response = provider
        .client
        .exchange_code(code)
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await;

    let token_response = match token_response {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[oidc] Token exchange failed: {e}");
            return (
                axum::http::StatusCode::BAD_GATEWAY,
                "Token exchange with identity provider failed",
            )
                .into_response();
        }
    };

    // 3. Verify and decode the ID token.
    let id_token = match token_response.id_token() {
        Some(t) => t,
        None => {
            return (
                axum::http::StatusCode::BAD_GATEWAY,
                "Identity provider returned no ID token",
            )
                .into_response();
        }
    };

    let claims = match id_token.claims(&provider.client.id_token_verifier(), &nonce) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[oidc] ID token verification failed: {e}");
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                "ID token verification failed",
            )
                .into_response();
        }
    };

    // 4. Extract identity claims.
    let subject = claims.subject().to_string();
    let provider_id_str = provider.id.clone(); // e.g. "1"

    let email = claims
        .email()
        .map(|e| e.to_string())
        .unwrap_or_else(|| format!("{subject}@unknown"));

    // preferred_username → fallback to email prefix.
    let username: String = claims
        .preferred_username()
        .map(|u| u.to_string())
        .unwrap_or_else(|| email.split('@').next().unwrap_or(&subject).to_string());

    // 5. Upsert the user row.
    //    On conflict (same provider + subject) we update the email so it stays fresh.
    //    The UNIQUE constraint on (oidc_provider, oidc_subject) set in migration 0002
    //    ensures we never create a duplicate identity.
    let user_id: Uuid = match sqlx::query_scalar!(
        r#"
        INSERT INTO users (username, email, oidc_provider, oidc_subject)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (oidc_provider, oidc_subject)
            DO UPDATE SET email = EXCLUDED.email, updated_at = NOW()
        RETURNING id
        "#,
        username,
        email,
        provider_id_str,
        subject
    )
    .fetch_one(&pool)
    .await
    {
        Ok(id) => id,
        Err(e) => {
            // The username might collide with an existing local user.
            // Re-try with a suffixed username derived from the subject.
            eprintln!("[oidc] User upsert failed (possibly duplicate username): {e}");
            let fallback_username = format!("{username}_{}", &subject[..8.min(subject.len())]);
            match sqlx::query_scalar!(
                r#"
                INSERT INTO users (username, email, oidc_provider, oidc_subject)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (oidc_provider, oidc_subject)
                    DO UPDATE SET email = EXCLUDED.email, updated_at = NOW()
                RETURNING id
                "#,
                fallback_username,
                email,
                provider_id_str,
                subject
            )
            .fetch_one(&pool)
            .await
            {
                Ok(id) => id,
                Err(e2) => {
                    eprintln!("[oidc] Fallback upsert also failed: {e2}");
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to provision user account",
                    )
                        .into_response();
                }
            }
        }
    };

    // 6. Create a local session cookie (same mechanism as password login).
    let token = match crate::server::auth::create_session(&pool, user_id).await {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[oidc] Session creation failed: {e}");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to create session",
            )
                .into_response();
        }
    };

    let mut cookie = tower_cookies::Cookie::new(crate::server::auth::SESSION_COOKIE, token);
    cookie.set_http_only(true);
    cookie.set_path("/");
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax); // Lax required for cross-site redirect
    cookies.add(cookie);

    axum::response::Redirect::temporary("/").into_response()
}

/// Query parameters received on the OIDC callback URL.
#[derive(serde::Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}
