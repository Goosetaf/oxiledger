//! The parts of the app that other programs in this repository can use.

#[cfg(feature = "server")]
pub mod bank_sync;
pub mod models;
#[cfg(feature = "server")]
pub mod server;
