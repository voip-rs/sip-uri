//! RFC URI equivalence, apart from `Eq`, which is canonical identity.
//!
//! The rules follow each scheme's RFC. `transport` must match whenever
//! either URI has it, though RFC 3261 §19.1.4's uri-parameter list names
//! only user, ttl, method and maddr: the same section says a URI omitting a
//! component with a default value, the transport-parameter included, does
//! not match one stating it, and lists `sip:bob@biloxi.com` and
//! `sip:bob@biloxi.com;transport=udp` as not equivalent.

use sip_uri_types::{Headers, Params, SipUri, TelUri, Uri, UrnUri};

mod sealed {
    pub trait Sealed {}
}

/// URI equivalence as each scheme's RFC defines it, apart from `Eq`, which
/// is canonical-structural identity.
///
/// - [`SipUri`], RFC 3261 §19.1.4: scheme, userinfo (user, user-params and
///   password, case-sensitive), host and port must match, a port left out
///   never matching one written, even 5060. A `user`, `ttl`, `method`,
///   `maddr` or `transport` param must match when either URI has it; any
///   other param is compared only when both have it. Param names and values
///   compare case-insensitively and in any order. Headers match by
///   [`ordered_headers_equivalent`]: rows sharing a name in order, values
///   exactly. Fragments, which RFC 3261 does not define, must match exactly.
/// - [`TelUri`], RFC 3966 §4: the numbers match with visual separators
///   removed, case-insensitively; every param must be in both, in any order,
///   names and values case-insensitive. An `ext` value, or a `phone-context`
///   that is a global number, has its visual separators removed; a domain
///   `phone-context` compares case-insensitively. Fragments must match
///   exactly.
/// - [`UrnUri`], RFC 8141 §3: the NID and NSS match on canonical form; the
///   r-, q- and f-components are ignored.
/// - [`Uri`] compares within one variant, and [`Uri::Other`] by `Eq`.
///
/// A param name that appears more than once is compared by its first value.
///
/// SIP equivalence is not transitive: `sip:alice@example.com` is equivalent
/// to both `;x=1` and `;x=2` forms, which are not equivalent to each other.
/// It decides whether two URIs name the same resource, so it cannot key a
/// map or group URIs into classes; `Eq` and `Hash` can.
///
/// ```
/// use sip_uri::{SipUri, UriEquivalence, UriParse};
///
/// let a = SipUri::parse("sip:%61lice@example.com;transport=TCP").unwrap();
/// let b = SipUri::parse("sip:alice@EXAMPLE.com;Transport=tcp").unwrap();
/// assert_ne!(a, b);
/// assert!(a.equivalent(&b));
/// ```
pub trait UriEquivalence: sealed::Sealed {
    /// Whether `self` and `other` are equivalent URIs.
    fn equivalent(&self, other: &Self) -> bool;
}

impl sealed::Sealed for SipUri {}
impl sealed::Sealed for TelUri {}
impl sealed::Sealed for UrnUri {}
impl sealed::Sealed for Uri {}

/// SIP params compared even when only one URI carries them.
const SIP_PARAMS_ALWAYS_COMPARED: [&str; 5] = ["user", "ttl", "method", "maddr", "transport"];

impl UriEquivalence for SipUri {
    fn equivalent(&self, other: &Self) -> bool {
        self.scheme() == other.scheme()
            && self.user() == other.user()
            && self.user_params() == other.user_params()
            && self.password() == other.password()
            && self.host() == other.host()
            && self.port() == other.port()
            && sip_params_equivalent(self.params(), other.params())
            && ordered_headers_equivalent(self.headers(), other.headers())
            && self.fragment() == other.fragment()
    }
}

fn values_equivalent(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (a, b) => a == b,
    }
}

fn sip_params_equivalent(a: &Params, b: &Params) -> bool {
    let one_way = |x: &Params, y: &Params| {
        x.iter()
            .all(|(name, _)| match (x.get(name), y.get(name)) {
                (Some(xv), Some(yv)) => values_equivalent(xv, yv),
                _ => !SIP_PARAMS_ALWAYS_COMPARED
                    .iter()
                    .any(|p| p.eq_ignore_ascii_case(name)),
            })
    };
    one_way(a, b) && one_way(b, a)
}

/// URI headers equivalent by RFC 3261 §7.3.1 row order with exact values:
/// rows with different names in any order, rows sharing a name, compared
/// ASCII-case-insensitively, in order and with byte-equal canonical values.
///
/// This is [`UriEquivalence::equivalent`]'s header rule. It knows no header
/// field's own matching rules, which RFC 3261 §19.1.4 asks for and a SIP
/// header parser supplies.
///
/// ```
/// use sip_uri::{ordered_headers_equivalent, SipUri, UriParse};
///
/// let headers = |s: &str| SipUri::parse(s).unwrap().headers().clone();
/// let a = headers("sip:example.com?Route=a&Subject=x&Route=b");
/// assert!(ordered_headers_equivalent(&a, &headers("sip:example.com?subject=x&route=a&route=b")));
/// assert!(!ordered_headers_equivalent(&a, &headers("sip:example.com?Route=b&Subject=x&Route=a")));
/// ```
pub fn ordered_headers_equivalent(a: &Headers, b: &Headers) -> bool {
    fn values_named<'a>(list: &'a Headers, name: &str) -> impl Iterator<Item = Option<&'a str>> {
        let name = name.to_ascii_lowercase();
        list.iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(&name))
            .map(|(_, v)| v)
    }
    a.len() == b.len()
        && a.iter()
            .all(|(name, _)| values_named(a, name).eq(values_named(b, name)))
}

/// RFC 3966 `visual-separator`.
fn is_visual_separator(c: char) -> bool {
    matches!(c, '-' | '.' | '(' | ')')
}

fn digits_equivalent(a: &str, b: &str) -> bool {
    let digits = |s: &str| {
        s.chars()
            .filter(|c| !is_visual_separator(*c))
            .map(|c| c.to_ascii_lowercase())
            .collect::<String>()
    };
    digits(a) == digits(b)
}

/// A `phone-context` that is a domain compares as a host name, without
/// stripping its `-` and `.`.
fn tel_param_values_equivalent(name: &str, a: Option<&str>, b: Option<&str>) -> bool {
    let digits = |v: &str| {
        name.eq_ignore_ascii_case("ext")
            || (name.eq_ignore_ascii_case("phone-context") && v.starts_with('+'))
    };
    match (a, b) {
        (Some(a), Some(b)) if digits(a) && digits(b) => digits_equivalent(a, b),
        (a, b) => values_equivalent(a, b),
    }
}

fn tel_params_equivalent(a: &Params, b: &Params) -> bool {
    let one_way = |x: &Params, y: &Params| {
        x.iter()
            .all(|(name, _)| match (x.get(name), y.get(name)) {
                (Some(xv), Some(yv)) => tel_param_values_equivalent(name, xv, yv),
                _ => false,
            })
    };
    one_way(a, b) && one_way(b, a)
}

impl UriEquivalence for TelUri {
    fn equivalent(&self, other: &Self) -> bool {
        let numbers = match (self.number(), other.number()) {
            (Some(a), Some(b)) => digits_equivalent(a, b),
            (a, b) => a == b,
        };
        numbers
            && tel_params_equivalent(self.params(), other.params())
            && self.fragment() == other.fragment()
    }
}

impl UriEquivalence for UrnUri {
    fn equivalent(&self, other: &Self) -> bool {
        self.nid() == other.nid() && self.nss() == other.nss()
    }
}

impl UriEquivalence for Uri {
    fn equivalent(&self, other: &Self) -> bool {
        match (self, other) {
            (Uri::Sip(a), Uri::Sip(b)) => a.equivalent(b),
            (Uri::Tel(a), Uri::Tel(b)) => a.equivalent(b),
            (Uri::Urn(a), Uri::Urn(b)) => a.equivalent(b),
            (Uri::Other(a), Uri::Other(b)) => a == b,
            _ => false,
        }
    }
}
