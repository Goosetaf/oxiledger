/// Error types for the bank sync subsystem.
#[derive(Debug)]
pub enum BankSyncError {
    /// Network or HTTP-level error communicating with the provider.
    Http(String),
    /// The provider returned an authentication/authorization error.
    Auth(String),
    /// Rate limit exceeded.
    RateLimit,
    /// The requested resource was not found at the provider.
    NotFound(String),
    /// The provider returned a response we could not parse.
    Parse(String),
    /// A configuration value is missing or invalid (e.g. missing env var).
    Config(String),
    /// Any other provider-side error.
    Provider(String),
}

impl std::fmt::Display for BankSyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BankSyncError::Http(msg) => write!(f, "HTTP error: {msg}"),
            BankSyncError::Auth(msg) => write!(f, "Authentication error: {msg}"),
            BankSyncError::RateLimit => write!(f, "Rate limit exceeded — please try again later"),
            BankSyncError::NotFound(msg) => write!(f, "Not found: {msg}"),
            BankSyncError::Parse(msg) => write!(f, "Failed to parse provider response: {msg}"),
            BankSyncError::Config(msg) => write!(f, "Configuration error: {msg}"),
            BankSyncError::Provider(msg) => write!(f, "Provider error: {msg}"),
        }
    }
}

impl std::error::Error for BankSyncError {}

impl From<BankSyncError> for dioxus::prelude::ServerFnError {
    fn from(err: BankSyncError) -> Self {
        dioxus::prelude::ServerFnError::new(err.to_string())
    }
}
