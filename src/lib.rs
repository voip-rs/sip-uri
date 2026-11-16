//! SIP/SIPS, tel:, and URN parser.
//!
//! Implements RFC 3261 §19/§25 (SIP-URI, SIPS-URI), RFC 3966 (tel-URI),
//! and RFC 8141 (URN) with hand-written parsing and per-component
//! percent-encoding. The value types come from [`sip_uri_types`] and are
//! re-exported here.
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
