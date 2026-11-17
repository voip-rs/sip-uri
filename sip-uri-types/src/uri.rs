//! The URI sum type and the opaque `Other` value.
//!
//! Converting a `Uri` into one variant's type fails with
//! [`KindMismatchError`], which hands the `Uri` back and names both kinds
//! but never its text, since a user part is often a phone number.
//!
//! `Other` keeps its text as sent except for the scheme, lowercased because
//! schemes compare case-insensitively, so `scheme()` reads alike across
//! variants and two spellings of one URI are equal. Only bytes that would
//! break a header line are escaped and nothing is decoded: an unknown
//! scheme's grammar is not ours to normalize.

use std::fmt;

use crate::canon;
use crate::sip_uri::Scheme;
use crate::sip_uri::SipUri;
use crate::tel_uri::TelUri;
use crate::urn_uri::UrnUri;

/// A URI: SIP/SIPS, tel, URN, or an opaque URI with an unrecognized scheme.
///
/// Parse text into one with sip-uri's `UriParse`.
///
/// The `Other` variant keeps text without a structure of its own (e.g. `http:`,
/// `https:`, `data:`, or text without a scheme) as sent, apart from a
/// lowercased scheme and escaped bytes that would break a header line,
/// decoding nothing, so header values like `Call-Info` keep non-SIP URIs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "lowercase")
)]
pub enum Uri {
    /// SIP or SIPS URI.
    Sip(SipUri),
    /// tel: URI.
    Tel(TelUri),
    /// URN (Uniform Resource Name).
    Urn(UrnUri),
    /// URI with an unrecognized scheme, or none.
    Other(OtherUri),
}

/// Text kept as sent except for its scheme, which is lowercased, and the
/// bytes [`OtherUri::new`] escapes.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct OtherUri {
    raw: String,
    scheme_end: Option<usize>,
}

impl OtherUri {
    /// `scheme:rest` with the scheme lowercased, or `rest` alone without one.
    ///
    /// In `rest`, control bytes, space, `<`, `>`, `"` and every byte of a
    /// non-ASCII character become uppercase `%XX`; nothing is decoded, and
    /// every other byte, `%` included, is kept as sent. An owned `rest`
    /// already in that form is held in place, the scheme and `:` written in
    /// front of it, without a copy when it has the spare capacity.
    ///
    /// Fails when there is neither scheme nor text, or when the scheme is
    /// outside the RFC 3986 grammar or has a URI type of its own.
    ///
    /// ```
    /// use sip_uri_types::{OtherUri, OtherUriError};
    ///
    /// let uri = OtherUri::new(Some("HTTPS"), "//example.com/a b").unwrap();
    /// assert_eq!(uri.as_str(), "https://example.com/a%20b");
    /// assert_eq!(OtherUri::new(Some("sip"), "x"), Err(OtherUriError::TypedScheme));
    /// ```
    #[inline]
    pub fn new(scheme: Option<&str>, rest: impl AsRef<str>) -> Result<Self, OtherUriError> {
        let rest = rest.as_ref();
        match scheme {
            None if rest.is_empty() => Err(OtherUriError::Empty),
            None => Ok(OtherUri {
                raw: canon::canonize_other(rest),
                scheme_end: None,
            }),
            Some(s) if !canon::is_scheme(s) => Err(OtherUriError::InvalidScheme),
            Some(s) if canon::is_known_scheme(s) => Err(OtherUriError::TypedScheme),
            Some(s) => {
                let mut raw = String::with_capacity(s.len() + 1 + rest.len());
                raw.push_str(s);
                raw.push(':');
                canon::push_other(&mut raw, rest);
                if let Some(scheme) = raw.get_mut(..s.len()) {
                    scheme.make_ascii_lowercase();
                }
                Ok(OtherUri {
                    raw,
                    scheme_end: Some(s.len()),
                })
            }
        }
    }

    /// The whole URI, with its scheme lowercased.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// The scheme, lowercase, or `None` when the text has none.
    pub fn scheme(&self) -> Option<&str> {
        self.scheme_end
            .map(|end| &self.raw[..end])
    }

    /// The text after the scheme's `:`, or the whole text without a scheme.
    pub fn rest(&self) -> &str {
        let start = self
            .scheme_end
            .map_or(0, |end| end + 1);
        &self.raw[start..]
    }
}

/// Why [`OtherUri::new`] refused its parts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OtherUriError {
    /// Neither a scheme nor text.
    Empty,
    /// The scheme is outside the RFC 3986 grammar.
    InvalidScheme,
    /// The scheme is `sip`, `sips`, `tel` or `urn`, which have their own type.
    TypedScheme,
}

impl fmt::Display for OtherUriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            OtherUriError::Empty => "other URI has neither scheme nor text",
            OtherUriError::InvalidScheme => "other URI scheme is outside RFC 3986 syntax",
            OtherUriError::TypedScheme => "other URI scheme has a URI type of its own",
        })
    }
}

impl std::error::Error for OtherUriError {}

impl fmt::Display for OtherUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// The text with every password written as `***`, as [`SipUri`]'s Debug
/// masks its password: the one in a `scheme://user:password@` userinfo, or
/// in other text whatever follows the second `:` of a run ending in `@`.
impl fmt::Debug for OtherUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut text = String::with_capacity(
            self.raw
                .len(),
        );
        let mut kept = 0;
        for (start, end) in self.password_spans() {
            text.push_str(&self.raw[kept..start]);
            text.push_str("***");
            kept = end;
        }
        text.push_str(&self.raw[kept..]);
        write!(f, "OtherUri({text:?})")
    }
}

impl OtherUri {
    /// Byte ranges of passwords, in order: a URL authority's, or for text
    /// that is not `scheme://`, the colon rule's.
    fn password_spans(&self) -> Vec<(usize, usize)> {
        if let Some(span) = self.url_password_span() {
            return vec![span];
        }
        let hierarchical = self
            .scheme_end
            .and_then(|end| {
                self.raw
                    .get(end + 1..)
            })
            .is_some_and(|rest| rest.starts_with("//"));
        if hierarchical {
            return Vec::new();
        }
        self.colon_password_spans()
    }

    /// In each run of text ending in `@`, whatever follows its second `:`, so
    /// `sip:u:pw@` masks `pw` while `mailto:alice@` and `a:b@` mask nothing.
    fn colon_password_spans(&self) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut start = 0;
        for (at, _) in self
            .raw
            .match_indices('@')
        {
            let second_colon = self.raw[start..at]
                .match_indices(':')
                .nth(1);
            if let Some((colon, _)) = second_colon {
                spans.push((start + colon + 1, at));
            }
            start = at + 1;
        }
        spans
    }

    fn url_password_span(&self) -> Option<(usize, usize)> {
        let authority_start = self.scheme_end? + "://".len();
        let authority = self
            .raw
            .get(self.scheme_end? + 1..)?
            .strip_prefix("//")?;
        let authority = authority
            .find(['/', '?', '#'])
            .map_or(authority, |end| &authority[..end]);
        let userinfo = &authority[..authority.rfind('@')?];
        let colon = userinfo.find(':')?;
        Some((
            authority_start + colon + 1,
            authority_start + userinfo.len(),
        ))
    }
}

impl Uri {
    /// If this is a SIP/SIPS URI, return a reference to it.
    pub fn as_sip(&self) -> Option<&SipUri> {
        match self {
            Uri::Sip(u) => Some(u),
            _ => None,
        }
    }

    /// If this is a tel: URI, return a reference to it.
    pub fn as_tel(&self) -> Option<&TelUri> {
        match self {
            Uri::Tel(u) => Some(u),
            _ => None,
        }
    }

    /// If this is a URN, return a reference to it.
    pub fn as_urn(&self) -> Option<&UrnUri> {
        match self {
            Uri::Urn(u) => Some(u),
            _ => None,
        }
    }

    /// If this is a URI with an unrecognized scheme, or none, return it.
    pub fn as_other(&self) -> Option<&OtherUri> {
        match self {
            Uri::Other(o) => Some(o),
            _ => None,
        }
    }

    /// Consume and return the inner SIP/SIPS URI, if any.
    pub fn into_sip(self) -> Option<SipUri> {
        match self {
            Uri::Sip(u) => Some(u),
            _ => None,
        }
    }

    /// Consume and return the inner tel: URI, if any.
    pub fn into_tel(self) -> Option<TelUri> {
        match self {
            Uri::Tel(u) => Some(u),
            _ => None,
        }
    }

    /// Consume and return the inner URN, if any.
    pub fn into_urn(self) -> Option<UrnUri> {
        match self {
            Uri::Urn(u) => Some(u),
            _ => None,
        }
    }

    /// Consume and return the URI with an unrecognized scheme, or none.
    pub fn into_other(self) -> Option<OtherUri> {
        match self {
            Uri::Other(o) => Some(o),
            _ => None,
        }
    }

    /// The scheme of this URI, lowercase: `None` for a `SipUri` or `Other`
    /// read without one, always present for tel: and URN, whose type implies
    /// it.
    pub fn scheme(&self) -> Option<&str> {
        match self {
            Uri::Sip(u) => u
                .scheme()
                .map(Scheme::as_str),
            Uri::Tel(_) => Some("tel"),
            Uri::Urn(_) => Some("urn"),
            Uri::Other(o) => o.scheme(),
        }
    }

    /// Extract the user/subscriber identifier from the URI.
    ///
    /// - `sip:`/`sips:` → user part before `@`
    /// - `tel:` → phone number (the dialable identifier)
    /// - `urn:` / other → `None`
    pub fn user(&self) -> Option<&str> {
        match self {
            Uri::Sip(u) => u.user(),
            Uri::Tel(u) => u.number(),
            Uri::Urn(_) | Uri::Other(_) => None,
        }
    }
}

impl From<SipUri> for Uri {
    fn from(u: SipUri) -> Self {
        Uri::Sip(u)
    }
}

impl From<TelUri> for Uri {
    fn from(u: TelUri) -> Self {
        Uri::Tel(u)
    }
}

impl From<UrnUri> for Uri {
    fn from(u: UrnUri) -> Self {
        Uri::Urn(u)
    }
}

impl From<OtherUri> for Uri {
    fn from(o: OtherUri) -> Self {
        Uri::Other(o)
    }
}

/// A [`Uri`] converted into a variant's type it does not hold.
///
/// Returned by the [`TryFrom<Uri>`] impls of [`SipUri`], [`TelUri`],
/// [`UrnUri`] and [`OtherUri`]. Display names the expected and the found
/// kind, never the URI text; its wording is for people and may change in a
/// minor release, so code matches on [`uri`](Self::uri) instead.
///
/// ```
/// use sip_uri_types::{SipUri, TelUri, Uri};
///
/// let uri = Uri::Tel(TelUri::new("+15551234567"));
/// let err = SipUri::try_from(uri.clone()).unwrap_err();
/// assert!(err.uri().as_tel().is_some());
/// assert_eq!(err.into_uri(), uri);
/// ```
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct KindMismatchError {
    expected: &'static str,
    uri: Uri,
}

/// Names the kinds only, as Display does: `unwrap()` on a conversion puts
/// this in the panic message.
impl fmt::Debug for KindMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KindMismatchError")
            .field("expected", &self.expected)
            .field("found", &kind(&self.uri))
            .finish()
    }
}

impl KindMismatchError {
    /// The URI that was not of the expected kind.
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// The URI that was not of the expected kind, handed back.
    pub fn into_uri(self) -> Uri {
        self.uri
    }
}

const SIP: &str = "a SIP URI";
const TEL: &str = "a tel: URI";
const URN: &str = "a URN";
const OTHER: &str = "an unrecognized URI";

fn kind(uri: &Uri) -> &'static str {
    match uri {
        Uri::Sip(_) => SIP,
        Uri::Tel(_) => TEL,
        Uri::Urn(_) => URN,
        Uri::Other(_) => OTHER,
    }
}

impl fmt::Display for KindMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {}, found {}", self.expected, kind(&self.uri))
    }
}

impl std::error::Error for KindMismatchError {}

/// `Uri` into a variant's type, a [`KindMismatchError`] when it is another.
macro_rules! try_from_uri {
    ($($variant:ident => $ty:ty, $kind:ident);*) => {$(
        impl TryFrom<Uri> for $ty {
            type Error = KindMismatchError;

            fn try_from(uri: Uri) -> Result<Self, KindMismatchError> {
                match uri {
                    Uri::$variant(u) => Ok(u),
                    uri => Err(KindMismatchError { expected: $kind, uri }),
                }
            }
        }
    )*};
}

try_from_uri!(
    Sip => SipUri, SIP;
    Tel => TelUri, TEL;
    Urn => UrnUri, URN;
    Other => OtherUri, OTHER
);

impl AsRef<str> for OtherUri {
    fn as_ref(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Uri::Sip(u) => fmt::Display::fmt(u, f),
            Uri::Tel(u) => fmt::Display::fmt(u, f),
            Uri::Urn(u) => fmt::Display::fmt(u, f),
            Uri::Other(o) => fmt::Display::fmt(o, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Host;

    #[test]
    fn debug_masks_every_password() {
        let other = |s: &str, r: &str| OtherUri::new(Some(s), r).unwrap();
        let cases = [
            (
                other("https", "//alice:secret@example.com/a?b"),
                r#"OtherUri("https://alice:***@example.com/a?b")"#,
            ),
            (
                other("https", "//alice@example.com/"),
                r#"OtherUri("https://alice@example.com/")"#,
            ),
            (
                other("https", "//example.com/a:b@c"),
                r#"OtherUri("https://example.com/a:b@c")"#,
            ),
            (
                other("mailto", "alice@example.com"),
                r#"OtherUri("mailto:alice@example.com")"#,
            ),
            (
                other("mailto", "alice:x@example.com"),
                r#"OtherUri("mailto:alice:***@example.com")"#,
            ),
            (
                OtherUri::new(None, "a:b@c").unwrap(),
                r#"OtherUri("a:b@c")"#,
            ),
            (
                OtherUri::new(None, "<sip:alice:secret@example.com>").unwrap(),
                r#"OtherUri("%3Csip:alice:***@example.com%3E")"#,
            ),
            (
                OtherUri::new(None, "sip:+15551234567:pw@example.com;x=a:b@c").unwrap(),
                r#"OtherUri("sip:+15551234567:***@example.com;x=a:b@c")"#,
            ),
            (
                OtherUri::new(None, r#""a@b" <sip:user:secret@example.com>"#).unwrap(),
                r#"OtherUri("%22a@b%22%20%3Csip:user:***@example.com%3E")"#,
            ),
            (
                other("mailto", "a@b,sip:user:secret@example.com"),
                r#"OtherUri("mailto:a@b,sip:user:***@example.com")"#,
            ),
            (
                OtherUri::new(None, "<sip:user:se:cret@example.com>").unwrap(),
                r#"OtherUri("%3Csip:user:***@example.com%3E")"#,
            ),
        ];
        for (uri, debug) in cases {
            assert_eq!(format!("{uri:?}"), debug);
        }
        let uri = Uri::Other(other("https", "//alice:secret@example.com/"));
        assert!(!format!("{uri:?}").contains("secret"));
    }

    #[test]
    fn kind_mismatch_is_an_error_that_hands_the_uri_back() {
        fn as_sip(uri: Uri) -> Result<SipUri, Box<dyn std::error::Error>> {
            Ok(SipUri::try_from(uri)?)
        }
        let sip =
            Uri::Sip(SipUri::new(Host::Hostname("example.com".into())).with_user("+15551234567"));
        let tel = Uri::Tel(TelUri::new("+15551234567"));
        let urn = Uri::Urn(UrnUri::new("service", "sos"));
        let other = Uri::Other(OtherUri::new(Some("http"), "//example.com/").unwrap());
        assert!(as_sip(sip.clone()).is_ok());
        let err = as_sip(tel.clone()).unwrap_err();
        assert!(
            !err.to_string()
                .contains("5551234567"),
            "{err}"
        );
        let debug = format!("{:?}", TelUri::try_from(sip.clone()).unwrap_err());
        assert_eq!(
            debug,
            r#"KindMismatchError { expected: "a tel: URI", found: "a SIP URI" }"#
        );

        let cases = [
            (
                SipUri::try_from(urn).err(),
                "expected a SIP URI, found a URN",
            ),
            (
                TelUri::try_from(other.clone()).err(),
                "expected a tel: URI, found an unrecognized URI",
            ),
            (
                UrnUri::try_from(sip).err(),
                "expected a URN, found a SIP URI",
            ),
            (
                OtherUri::try_from(tel).err(),
                "expected an unrecognized URI, found a tel: URI",
            ),
        ];
        for (err, message) in cases {
            assert_eq!(
                err.unwrap()
                    .to_string(),
                message
            );
        }
    }

    #[test]
    fn other_uri_refuses_known_and_invalid_schemes() {
        assert_eq!(
            OtherUri::new(Some("SIP"), "alice@example.com"),
            Err(OtherUriError::TypedScheme)
        );
        assert_eq!(
            OtherUri::new(Some("<sip"), "alice@example.com"),
            Err(OtherUriError::InvalidScheme)
        );
        assert_eq!(
            OtherUri::new(Some(""), "x"),
            Err(OtherUriError::InvalidScheme)
        );
        let other = OtherUri::new(Some("HTTPS"), "//example.com").unwrap();
        assert_eq!(other.as_str(), "https://example.com");
        assert_eq!(other.scheme(), Some("https"));
        let bare = OtherUri::new(None, "*").unwrap();
        assert_eq!(bare.scheme(), None);
        assert_eq!(bare.as_str(), "*");
        assert_eq!(bare.rest(), "*");
        assert_eq!(other.rest(), "//example.com");
        assert_eq!(OtherUri::new(None, ""), Err(OtherUriError::Empty));
        assert!(OtherUri::new(Some("x-test"), "").is_ok());
    }

    #[test]
    fn variants_convert_both_ways() {
        let host = || crate::Host::Hostname("example.com".into());
        let uris = [
            Uri::from(SipUri::new(host())),
            Uri::from(TelUri::new("+15551234567")),
            Uri::from(UrnUri::new("service", "sos")),
            Uri::from(OtherUri::new(Some("https"), "//example.com").unwrap()),
        ];
        for uri in uris {
            let sip = SipUri::try_from(uri.clone()).map(Uri::from);
            let tel = TelUri::try_from(uri.clone()).map(Uri::from);
            let urn = UrnUri::try_from(uri.clone()).map(Uri::from);
            let other = OtherUri::try_from(uri.clone()).map(Uri::from);
            let results = [sip, tel, urn, other];
            for result in &results {
                let back = result
                    .clone()
                    .unwrap_or_else(KindMismatchError::into_uri);
                assert_eq!(back, uri);
            }
            let matched = results
                .iter()
                .filter(|r| r.is_ok())
                .count();
            assert_eq!(matched, 1, "{uri}");
        }
        let other = OtherUri::new(None, "*").unwrap();
        assert_eq!(AsRef::<str>::as_ref(&other), other.as_str());
    }

    #[test]
    fn other_uri_escapes_what_breaks_a_header_line() {
        let other = OtherUri::new(Some("http"), "//x\r\nVia: <y> \"é\" %zz").unwrap();
        assert_eq!(
            other.as_str(),
            "http://x%0D%0AVia:%20%3Cy%3E%20%22%C3%A9%22%20%zz"
        );
        assert_eq!(OtherUri::new(Some("http"), other.rest()), Ok(other));
        assert_eq!(
            OtherUri::new(None, "a\u{7f}b")
                .unwrap()
                .as_str(),
            "a%7Fb"
        );
    }
}
