use dioxus::prelude::ServerFnError;

pub type StorageResult<T> = Result<T, StorageError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    UniqueViolation(String),
    ForeignKeyViolation(String),
    /// The write was refused because the data it relied on had changed.
    Conflict(String),
    Other(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::UniqueViolation(message)
            | StorageError::ForeignKeyViolation(message)
            | StorageError::Conflict(message)
            | StorageError::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<StorageError> for ServerFnError {
    fn from(err: StorageError) -> Self {
        ServerFnError::new(err.to_string())
    }
}
