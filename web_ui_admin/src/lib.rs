//! Redesigned Topcoat UI admin portal.
#![deny(missing_docs)]

/// Routed pages of the admin portal.
pub mod pages;

/// Port the admin portal listens on when `PORT` is unset.
pub const DEFAULT_PORT: u16 = 3012;
