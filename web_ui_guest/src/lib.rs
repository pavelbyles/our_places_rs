//! Redesigned Topcoat UI guest portal.
#![deny(missing_docs)]

/// Routed pages of the guest portal.
pub mod pages;

/// Port the guest portal listens on when `PORT` is unset.
pub const DEFAULT_PORT: u16 = 3010;
