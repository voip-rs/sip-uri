mod host;
mod params;
mod sip;
mod tel;
mod uri;
mod urn;

use crate::error::ParseError;
use crate::warning::Parsed;
use sip_uri_types::Host;
use sip_uri_types::SipUri;
use sip_uri_types::TelUri;
use sip_uri_types::Uri;
use sip_uri_types::UrnUri;

mod sealed {
    pub trait Sealed {}
}

/// Parsing for the URI and host types.
///
/// All three methods run one parser. The lenient ones fail only on empty
/// input, a scheme belonging to another type, for [`Host`] input with no
/// readable host, and for [`SipUri`] input that leaves no component;
/// [`UriParse::parse_strict`] also fails on the first grammar breach.
///
/// - [`Uri`] accepts everything but empty input. Input without a scheme, or
///   with one outside the RFC 3986 grammar such as a URI still wrapped in
///   `<>`, is kept as [`Uri::Other`] with a warning.
/// - [`SipUri`], [`TelUri`] and [`UrnUri`] also reject another type's scheme.
/// - [`Host`] accepts a hostname, a bare IPv4 or IPv6 address, and a
///   bracketed IPv6 reference, but not bracketed IPv4, since brackets are the
///   `IPv6reference` production. What a URI would put after the host is
///   dropped with a `TrailingContent` warning, and input with no readable
///   host is the error.
///
/// ```
/// use sip_uri::{SipUri, UriParse, WarningCode};
///
/// let uri = SipUri::parse("sip:alice@example.com").unwrap();
/// assert_eq!(uri.user(), Some("alice"));
///
/// let parsed = SipUri::parse_with_warnings("sip:example.com:+5060").unwrap();
/// assert_eq!(parsed.value.port(), Some(5060));
/// assert!(parsed.warnings.iter().any(|w| w.code == WarningCode::SignedPort));
/// assert!(SipUri::parse_strict("sip:example.com:+5060").is_err());
///
/// use sip_uri::Host;
///
/// assert!(Host::parse("2001:db8::1").is_ok());
/// assert!(Host::parse("[2001:db8::1]").is_ok());
/// assert!(Host::parse("[198.51.100.1]").is_err());
/// let parsed = Host::parse_with_warnings("example.com:5060").unwrap();
/// assert!(parsed.warnings.iter().any(|w| w.code == WarningCode::TrailingContent));
/// ```
pub trait UriParse: Sized + sealed::Sealed {
    /// Parse, reporting accepted grammar breaches beside the value.
    fn parse_with_warnings(input: &str) -> Result<Parsed<Self>, ParseError>;

    /// Parse, discarding the warnings.
    fn parse(input: &str) -> Result<Self, ParseError> {
        Self::parse_with_warnings(input).map(|parsed| parsed.value)
    }

    /// Parse, rejecting any grammar breach as [`ParseError::NonConformant`].
    fn parse_strict(input: &str) -> Result<Self, ParseError> {
        Self::parse_with_warnings(input)?.into_strict()
    }
}

macro_rules! impl_uri_parse {
    ($($ty:ty => $parse:path),* $(,)?) => {$(
        impl sealed::Sealed for $ty {}

        impl UriParse for $ty {
            fn parse_with_warnings(input: &str) -> Result<Parsed<Self>, ParseError> {
                $parse(input)
            }
        }
    )*};
}

impl_uri_parse! {
    Uri => uri::parse,
    SipUri => sip::parse,
    TelUri => tel::parse,
    UrnUri => urn::parse,
    Host => host::parse,
}
