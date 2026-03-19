#[cfg(feature = "server")]
pub use dioxus_server::ServerFnError;

#[cfg(not(feature = "server"))]
#[derive(Debug)]
pub struct ServerFnError(String);

#[cfg(not(feature = "server"))]
impl ServerFnError {
    pub fn new<S: Into<String>>(s: S) -> Self {
        ServerFnError(s.into())
    }
}

#[cfg(not(feature = "server"))]
impl std::fmt::Display for ServerFnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ServerFnError: {}", self.0)
    }
}

#[cfg(not(feature = "server"))]
impl std::error::Error for ServerFnError {}
