use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stripped-down user info safe to send to the client (no password hash).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub email: String,
}

/// Request body for registering a new local account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// Request body for logging in with local credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}
