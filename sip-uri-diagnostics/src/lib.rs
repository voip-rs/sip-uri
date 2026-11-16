//! The warning and error vocabulary [sip-uri](https://docs.rs/sip-uri)
//! reports SIP/SIPS, tel: and URN URI parses in.
//!
//! A crate that hands sip-uri's diagnostics to its own callers names these
//! types in its public API, so they live in a 1.x crate of their own rather
//! than in sip-uri's 0.x one. sip-uri re-exports every item at its root
//! unchanged.
//!
//! # Contract
//!
//! This crate only grows. A new [`WarningCode`] or [`Component`] is a minor
//! release, which the `#[non_exhaustive]` on every enum allows; removing or
//! renaming one, or changing an [`as_str`](WarningCode::as_str) name, is
//! breaking.
//!
//! Nothing here names or quotes input text. A [`ParseWarning`] says which
//! component broke, how, at which byte offset and whether the value
//! survived, and every `Display` is built from those alone: the user part is
//! often a phone number, and warnings and errors are what consumers log.
//!
//! The minimum supported Rust version is 1.70. Raising it is a minor
//! release.
//!
//! ```
//! use sip_uri_diagnostics::{Component, ParseError, ParseWarning, WarningCode};
//!
//! let w = ParseWarning::new(Component::Port, WarningCode::SignedPort, Some(16));
//! assert_eq!(
//!     ParseError::NonConformant(w).to_string(),
//!     "non-conformant URI: port: signed port at byte 16"
//! );
//! ```

#![forbid(unsafe_code)]

mod error;
mod warning;

pub use error::ParseError;
pub use warning::{Component, ParseWarning, WarningCode, WarningKind};

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
