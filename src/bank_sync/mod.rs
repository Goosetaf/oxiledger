/// Bank sync subsystem.
///
/// This module provides a provider-agnostic abstraction for syncing bank
/// transactions. The [`provider::BankSyncProvider`] trait defines the interface
/// that every provider must implement. Add new providers by creating a new
/// module and implementing the trait — no other code needs to change.
///
/// Currently implemented providers:
/// - [`enable_banking`] — Enable Banking (https://enablebanking.com)
pub mod error;
pub mod provider;

#[cfg(feature = "server")]
pub mod enable_banking;

#[cfg(feature = "server")]
pub use enable_banking::EnableBankingProvider;
