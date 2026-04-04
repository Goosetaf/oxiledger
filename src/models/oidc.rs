use serde::{Deserialize, Serialize};

/// Minimal provider description sent to the client so it can render login buttons.
/// This intentionally contains no secrets — just display metadata and a stable ID.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OidcProviderInfo {
    /// Stable numeric ID derived from the env-var prefix (e.g. `1` for `OIDC_1_*`).
    pub id: String,
    /// Human-readable display name shown on the login button, e.g. "Acme SSO".
    pub name: String,
}
