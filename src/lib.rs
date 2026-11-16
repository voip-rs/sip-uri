//! SIP/SIPS, tel:, and URN parser.
//!
//! Implements RFC 3261 §19/§25 (SIP-URI, SIPS-URI), RFC 3966 (tel-URI),
//! and RFC 8141 (URN) with hand-written parsing and per-component
//! percent-encoding. The value types come from [`sip_uri_types`] and are
//! re-exported here.
//!
//! A crate that only passes URIs around depends on sip-uri-types, whose API
//! changes only when a value's identity does. This crate holds the policy
//! that moves more freely: parsing, warnings, redaction and equivalence.
//!
//! # Parsing and warnings
//!
//! [`UriParse`] parses text in three modes on one path.
//! [`parse_with_warnings`](UriParse::parse_with_warnings) returns the value
//! with every grammar breach it accepted as a [`ParseWarning`], which names
//! the component, a [`WarningCode`] and the byte offset but never the text.
//! [`parse`](UriParse::parse) drops the warnings, and
//! [`parse_strict`](UriParse::parse_strict) fails on the first one. A lenient
//! parse fails only when there is nothing to read or the scheme names
//! another kind of URI; see [`ParseError`].
//!
//! # Logging and comparison
//!
//! `Display` writes the user part and password. Logs use
//! [`UriRedact::redacted`] with a [`Redaction`], which masks every identity
//! by default and is relaxed per deployment.
//!
//! `==` is canonical identity: param order and name case count.
//! [`UriEquivalence`] is the RFC comparison, which is not transitive.
//!
//! # Features and MSRV
//!
//! `serde` serializes each value as its parts, through sip-uri-types, and
//! adds the `serde_str` module for fields that hold a URI as text. The minimum
//! supported Rust version is 1.70, 1.71 with `serde`; raising it is a minor
//! release.
//!
//! # Examples
//!
//! ```
//! use sip_uri::{SipUri, UriParse};
//!
//! let uri = SipUri::parse("sip:alice@example.com;transport=tcp").unwrap();
//! assert_eq!(uri.user(), Some("alice"));
//! assert_eq!(uri.param("transport"), Some(Some("tcp")));
//! ```
//!
//! [`UriParse`] parses every URI type the same way:
//!
//! ```
//! use sip_uri::{TelUri, Uri, UriParse, UrnUri};
//!
//! // Parse a tel: URI
//! let tel = TelUri::parse("tel:+15551234567;cpc=ordinary").unwrap();
//! assert_eq!(tel.number(), Some("+15551234567"));
//! assert!(tel.is_global());
//!
//! // Parse a URN
//! let urn = UrnUri::parse("urn:isbn:0451450523").unwrap();
//! assert_eq!(urn.nid(), Some("isbn"));
//! assert_eq!(urn.nss(), Some("0451450523"));
//!
//! // Dispatch on URI type
//! let uri = Uri::parse("urn:isbn:0451450523").unwrap();
//! match uri {
//!     Uri::Sip(sip) => println!("SIP: {sip}"),
//!     Uri::Tel(tel) => println!("Tel: {tel}"),
//!     Uri::Urn(urn) => println!("URN: {urn}"),
//!     Uri::Other(raw) => println!("other: {raw}"),
//! }
//! ```

#![forbid(unsafe_code)]

mod equivalence;
mod error;
mod grammar;
mod parser;
mod redact;
#[cfg(feature = "serde")]
pub mod serde_str;
mod warning;

pub use equivalence::UriEquivalence;
pub use error::ParseError;
pub use parser::UriParse;
pub use redact::{HeaderMask, OpaqueMask, Redacted, Redaction, UriRedact, UserMask};
pub use sip_uri_types;
pub use sip_uri_types::encoding;
pub use sip_uri_types::{
    AssignedName, Bare, Headers, Host, Hostname, KindMismatch, OtherUri, OtherUriError, Pairs,
    Params, Scheme, SipUri, SipUriParts, TelUri, TelUriParts, Uri, UrnUri, UrnUriParts, UserHost,
    UserParams,
};
pub use warning::{Component, ParseWarning, Parsed, WarningCode, WarningKind};

#[cfg(all(doctest, feature = "serde"))]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../docs/migrating-from-0.2.md")]
struct MigrationGuideDoctests;
