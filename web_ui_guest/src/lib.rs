//! Redesigned Topcoat UI guest portal.
#![deny(missing_docs)]

/// The `/` layout wrapping every page in the guest shell.
pub mod layout;
/// Routed pages of the guest portal.
pub mod pages;
/// Stay price arithmetic for quotes.
pub mod quote;
/// Shared helpers for guest pages (session, service tokens, shell).
pub mod support;

/// Port the guest portal listens on when `PORT` is unset.
pub const DEFAULT_PORT: u16 = 3010;
