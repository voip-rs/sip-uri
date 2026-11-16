//! SIP/SIPS, tel: and URN URI values, each component held in canonical form.
//!
//! Parse text into these types with [sip-uri](https://docs.rs/sip-uri).
//!
//! These types were sip-uri's own until 0.2; code written against them there
//! migrates with sip-uri's
//! [guide](https://docs.rs/sip-uri/latest/sip_uri/migrating_from_0_2/index.html).
//!
//! The `serde` feature serializes each value as its parts, and deserializes
//! them through the same canonizing constructor as [`From`] a parts struct.
//! A [`Host`] reads a hostname through [`Host::from_hostname`]. The shape is
//! under the same semver as the fields: a field added to a parts struct is a
//! minor release, missing fields take their defaults, unknown ones are
//! ignored, and a SIP URI read without `scheme` has none. That compatibility
//! needs a self-describing format such as JSON, YAML or CBOR: in a
//! positional one such as bincode or postcard, an added field changes the
//! encoding, so data stored that way does not survive a minor release.
//!
//! ```
//! use sip_uri_types::{Host, SipUri, TelUri};
//!
//! let uri = SipUri::new(Host::Hostname("example.com".into()))
//!     .with_user("+15551234567")
//!     .with_param("user", Some("phone"));
//! assert_eq!(uri.to_string(), "sip:+15551234567@example.com;user=phone");
//!
//! let tel = TelUri::new("+1555 123");
//! assert_eq!(tel.number(), Some("+1555%20123"));
//! ```
//!
//! # Canonical form
//!
//! Each component keeps its conformant characters literal, decodes escapes
//! of unreserved characters only, and holds every other byte as an uppercase
//! `%XX`, whichever path built the value. A tel: number, a fragment and a
//! URN component decode no escape at all. An escaped reserved character
//! stays escaped, so `%2B1` and `+1` are distinct users.
//!
//! Builders and parts structs take URI text, not logical values: a well-formed
//! `%XX` in it is read as an escape, never re-encoded. `with_user("%2B1")`
//! holds `%2B1`. [`encoding`] has one encoder and one decoder per component
//! a builder takes as text, between logical bytes and that text.
//!
//! Hostnames are lowercased as ASCII only, with no IDNA and no non-ASCII case
//! folding. An IPv6 address renders as [`std::net::Ipv6Addr`] displays it,
//! and an IPv6 zone ID is not representable.
//!
//! # Equality
//!
//! `Eq` and `Hash` compare the canonical form component by component. That
//! is identity, never RFC 3261 §19.1.4, RFC 3966 §4 or RFC 8141 §3
//! equivalence: param order, param and header name case, a tel: number's
//! visual separators, a hostname's trailing dot and a URN's r-, q- and
//! f-components all count. RFC equivalence is sip-uri's `UriEquivalence`.
//!
//! [`UriHeaderEquivalence`] is the interface by which that comparison
//! matches two [`Headers`] sets, which a SIP header parser implements with
//! each field's own rules; [`ordered_headers_equivalent`] is the default.
//!
//! # Stability
//!
//! The URI kinds with their own type are fixed: a URI with any other scheme
//! is [`Uri::Other`]. [`Uri`] and [`Scheme`] are therefore exhaustive.
//!
//! The minimum supported Rust version is 1.70, 1.71 with the `serde`
//! feature. Raising it is a minor release.

#![deny(unsafe_code)]
#![warn(clippy::undocumented_unsafe_blocks)]

mod canon;
mod equivalence;
mod host;
mod params;
#[cfg(feature = "serde")]
mod serde_impls;
mod sip_uri;
mod tel_uri;
mod uri;
mod urn_uri;

pub use canon::encoding;
pub use equivalence::{ordered_headers_equivalent, UriHeaderEquivalence};
pub use host::{Bare, Host, Hostname};
pub use params::{CapacityError, Headers, Pairs, Params, UserParams};
pub use sip_uri::{Scheme, SipUri, SipUriParts, UserHost};
pub use tel_uri::{TelUri, TelUriParts};
pub use uri::{KindMismatch, OtherUri, OtherUriError, Uri};
pub use urn_uri::{AssignedName, UrnUri, UrnUriParts};

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

#[cfg(test)]
mod tests {
    use super::*;

    fn is_send_sync_unpin<T: Send + Sync + Unpin>() {}

    #[test]
    fn data_types_are_send_sync_unpin() {
        is_send_sync_unpin::<Uri>();
        is_send_sync_unpin::<SipUri>();
        is_send_sync_unpin::<SipUriParts>();
        is_send_sync_unpin::<TelUri>();
        is_send_sync_unpin::<TelUriParts>();
        is_send_sync_unpin::<UrnUri>();
        is_send_sync_unpin::<UrnUriParts>();
        is_send_sync_unpin::<OtherUri>();
        is_send_sync_unpin::<OtherUriError>();
        is_send_sync_unpin::<KindMismatch>();
        is_send_sync_unpin::<Host>();
        is_send_sync_unpin::<Hostname>();
        is_send_sync_unpin::<Bare<'static>>();
        is_send_sync_unpin::<Scheme>();
        is_send_sync_unpin::<Params>();
        is_send_sync_unpin::<UserParams>();
        is_send_sync_unpin::<Headers>();
        is_send_sync_unpin::<Pairs<'static>>();
        is_send_sync_unpin::<UserHost<'static>>();
        is_send_sync_unpin::<AssignedName<'static>>();
    }
}
