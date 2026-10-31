//! Redesigned Topcoat UI admin portal.
#![deny(missing_docs)]

/// The `/` layout wrapping every page in the admin shell.
pub mod layout;
/// Routed pages of the admin portal.
pub mod pages;
/// Shared helpers for admin pages (session, service tokens, scoping).
pub mod support;

/// Port the admin portal listens on when `PORT` is unset.
pub const DEFAULT_PORT: u16 = 3012;
