//! RFC grammar classes deciding warnings, and the splits the parser makes.
//!
//! The `is_*` predicates are the source of truth for each character set.
//! Scans read them through `CLASSES`, a byte table built from them at
//! compile time with one bit per set, so a byte's membership is one load.
//! A new set is a predicate first, then a bit and a row in the table and in
//! its test, which checks every byte of every set against its predicate.
//! These sets decide warnings only; sip-uri-types keeps its own literal
//! sets for canonical form.

/// RFC 3986 §2: `unreserved / reserved`, every character a URI of any
/// scheme may hold outside a `%XX` escape. A character-set check only: a
/// delimiter in the wrong place, such as a second `#`, passes.
const fn is_rfc3986_char(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b':'
                | b'/'
                | b'?'
                | b'#'
                | b'['
                | b']'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
        )
}

/// RFC 3261 §25: `unreserved = alphanum / mark`
/// `mark = "-" / "_" / "." / "!" / "~" / "*" / "'" / "(" / ")"`
const fn is_unreserved(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        )
}

/// RFC 3261 §25: `user-unreserved = "&" / "=" / "+" / "$" / "," / ";" / "?" / "/"`
const fn is_user_unreserved(c: u8) -> bool {
    matches!(c, b'&' | b'=' | b'+' | b'$' | b',' | b';' | b'?' | b'/')
}

/// RFC 3261 §25: `param-unreserved = "[" / "]" / "/" / ":" / "&" / "+" / "$"`
const fn is_param_unreserved(c: u8) -> bool {
    matches!(c, b'[' | b']' | b'/' | b':' | b'&' | b'+' | b'$')
}

/// RFC 3261 §25: `hnv-unreserved = "[" / "]" / "/" / "?" / ":" / "+" / "$"`
const fn is_hnv_unreserved(c: u8) -> bool {
    matches!(c, b'[' | b']' | b'/' | b'?' | b':' | b'+' | b'$')
}

/// Characters allowed unescaped in the SIP user component:
/// unreserved + user-unreserved
const fn is_user_char(c: u8) -> bool {
    is_unreserved(c) || is_user_unreserved(c)
}

/// Characters allowed unescaped in the password component:
/// unreserved + "&" / "=" / "+" / "$" / ","
const fn is_password_char(c: u8) -> bool {
    is_unreserved(c) || matches!(c, b'&' | b'=' | b'+' | b'$' | b',')
}

/// `paramchar` as RFC 3261 §25 writes it: unreserved + param-unreserved.
const fn is_param_strict(c: u8) -> bool {
    is_unreserved(c) || is_param_unreserved(c)
}

/// RFC 3966: `pname = 1*( alphanum / "-" )`
const fn is_tel_pname_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-'
}

/// RFC 3966: `paramchar = param-unreserved / unreserved / pct-encoded`, whose
/// `param-unreserved` matches RFC 3261's.
const fn is_tel_paramchar(c: u8) -> bool {
    is_param_strict(c)
}

/// Characters allowed unescaped in header names/values:
/// unreserved + hnv-unreserved
const fn is_hnv_char(c: u8) -> bool {
    is_unreserved(c) || is_hnv_unreserved(c)
}

/// RFC 3986 pchar: `unreserved / pct-encoded / sub-delims / ":" / "@"`
const fn is_pchar(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
        )
}

/// RFC 8141: `NSS = pchar *(pchar / "/")`.
const fn is_nss_char(c: u8) -> bool {
    is_pchar(c) || c == b'/'
}

/// RFC 8141: r-, q- and f-components are `*( pchar / "/" / "?" )`.
const fn is_rqf_char(c: u8) -> bool {
    is_pchar(c) || matches!(c, b'/' | b'?')
}

/// A character set, one bit of [`CLASSES`] each.
#[derive(Clone, Copy)]
pub(crate) struct CharClass(u16);

pub(crate) const RFC3986: CharClass = CharClass(1 << 0);
pub(crate) const USER: CharClass = CharClass(1 << 1);
pub(crate) const PASSWORD: CharClass = CharClass(1 << 2);
pub(crate) const PARAM: CharClass = CharClass(1 << 3);
pub(crate) const TEL_PNAME: CharClass = CharClass(1 << 4);
pub(crate) const TEL_PARAMCHAR: CharClass = CharClass(1 << 5);
pub(crate) const HNV: CharClass = CharClass(1 << 6);
pub(crate) const URN_NSS: CharClass = CharClass(1 << 7);
pub(crate) const URN_RQF: CharClass = CharClass(1 << 8);

const fn bit(class: CharClass, member: bool) -> u16 {
    if member {
        class.0
    } else {
        0
    }
}

/// The sets each byte belongs to, built from the predicates.
const CLASSES: [u16; 256] = {
    let mut table = [0; 256];
    let mut i = 0;
    while i < table.len() {
        let c = i as u8;
        table[i] = bit(RFC3986, is_rfc3986_char(c))
            | bit(USER, is_user_char(c))
            | bit(PASSWORD, is_password_char(c))
            | bit(PARAM, is_param_strict(c))
            | bit(TEL_PNAME, is_tel_pname_char(c))
            | bit(TEL_PARAMCHAR, is_tel_paramchar(c))
            | bit(HNV, is_hnv_char(c))
            | bit(URN_NSS, is_nss_char(c))
            | bit(URN_RQF, is_rqf_char(c));
        i += 1;
    }
    table
};

impl CharClass {
    pub(crate) fn holds(self, b: u8) -> bool {
        CLASSES[b as usize] & self.0 != 0
    }
}

pub(crate) const ESCAPE_LEN: usize = 3;

/// Validate that a string contains only valid percent-encoded or allowed characters.
/// Returns `Err` with the position of the first invalid character.
pub(crate) fn validate_pct_encoded(input: &str, allowed: CharClass) -> Result<(), usize> {
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 < bytes.len()
                && bytes[i + 1].is_ascii_hexdigit()
                && bytes[i + 2].is_ascii_hexdigit()
            {
                i += ESCAPE_LEN;
                continue;
            }
            return Err(i);
        }
        if !allowed.holds(bytes[i]) {
            return Err(i);
        }
        i += 1;
    }

    Ok(())
}

/// How an input starts, before any type-specific parsing.
pub(crate) enum SchemeSplit<'a> {
    /// A scheme and the text after its `:`.
    Named(&'a str, &'a str),
    /// A `:` whose prefix is not RFC 3986 scheme syntax, e.g. `<sip:…>`.
    Invalid,
    /// No `:`, or `host:port` with the scheme left out.
    Absent,
}

/// Schemes this crate parses; always read as schemes, even before digits.
const KNOWN_SCHEMES: [&str; 4] = ["sip", "sips", "tel", "urn"];

pub(crate) fn split_scheme(input: &str) -> SchemeSplit<'_> {
    let Some((scheme, rest)) = input.split_once(':') else {
        return SchemeSplit::Absent;
    };
    if KNOWN_SCHEMES
        .iter()
        .any(|k| scheme.eq_ignore_ascii_case(k))
    {
        return SchemeSplit::Named(scheme, rest);
    }
    if !is_rfc3986_scheme(scheme) {
        // `user@host:port` and `[v6]:port` reach their first `:` inside the authority.
        if scheme.contains('@') || scheme.starts_with('[') {
            return SchemeSplit::Absent;
        }
        return SchemeSplit::Invalid;
    }
    // `example.com:5060` is a host and port with no scheme, not scheme `example.com`.
    // The port ends only at bytes an `Other` keeps literal, so its Display reads the same.
    let port = rest
        .split([';', '?', '#', '/'])
        .next()
        .unwrap_or_default();
    if !port.is_empty()
        && port
            .bytes()
            .all(|b| b.is_ascii_digit())
    {
        return SchemeSplit::Absent;
    }
    SchemeSplit::Named(scheme, rest)
}

/// RFC 3986 §3.1: `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
fn is_rfc3986_scheme(s: &str) -> bool {
    let mut bytes = s.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

/// Find the `@` delimiter that separates userinfo from hostport in a SIP URI.
///
/// Uses the sofia-sip two-phase algorithm (url.c:616-626): scan to the first
/// `@/;?#`, then take the first `@` from there on. Everything before it is
/// userinfo. Without userinfo, a literal `@` in a param or header value is
/// therefore taken as the delimiter; see the notes of `parser::sip`.
pub(crate) fn find_userinfo_at(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;

    // Phase 1: scan to first "@/;?#"
    while i < bytes.len() && !matches!(bytes[i], b'@' | b'/' | b';' | b'?' | b'#') {
        i += 1;
    }

    // Phase 2: from that point, scan for '@'
    while i < bytes.len() {
        if bytes[i] == b'@' {
            return Some(i);
        }
        i += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use sip_uri_types::{OtherUri, OtherUriError};

    fn split_kind(input: &str) -> &'static str {
        match split_scheme(input) {
            SchemeSplit::Named(..) => "named",
            SchemeSplit::Invalid => "invalid",
            SchemeSplit::Absent => "absent",
        }
    }

    #[test]
    fn port_rule_reads_the_same_on_canonical_text() {
        for b in 0x00..=0x7Fu8 {
            for raw in [
                format!("l:1{}", b as char),
                format!("example.com:5060{}x", b as char),
            ] {
                let Ok(canonical) = OtherUri::new(None, &raw) else {
                    continue;
                };
                let canonical = canonical.to_string();
                assert_eq!(
                    split_kind(&raw),
                    split_kind(&canonical),
                    "byte {b:#04x}: {raw:?} vs {canonical:?}"
                );
            }
        }
    }

    #[test]
    fn class_table_matches_predicates() {
        type Predicate = fn(u8) -> bool;
        let classes: [(CharClass, Predicate); 9] = [
            (RFC3986, is_rfc3986_char),
            (USER, is_user_char),
            (PASSWORD, is_password_char),
            (PARAM, is_param_strict),
            (TEL_PNAME, is_tel_pname_char),
            (TEL_PARAMCHAR, is_tel_paramchar),
            (HNV, is_hnv_char),
            (URN_NSS, is_nss_char),
            (URN_RQF, is_rqf_char),
        ];
        for (class, predicate) in classes {
            for b in 0..=u8::MAX {
                assert_eq!(class.holds(b), predicate(b), "{b:#04x}");
            }
        }
    }

    #[test]
    fn unreserved_chars() {
        for c in b"abcABC019-_.!~*'()" {
            assert!(is_unreserved(*c), "expected unreserved: {:?}", *c as char);
        }
        for c in b"@:;/?#[]&=+$,\" " {
            assert!(!is_unreserved(*c), "expected reserved: {:?}", *c as char);
        }
    }

    /// Every scheme of up to four letters, the length of each known one.
    fn short_schemes() -> impl Iterator<Item = String> {
        (1..=4u32).flat_map(|len| {
            (0..26usize.pow(len)).map(move |mut n| {
                let mut s = String::new();
                for _ in 0..len {
                    s.push(char::from(b'a' + (n % 26) as u8));
                    n /= 26;
                }
                s
            })
        })
    }

    #[test]
    fn known_schemes_are_the_ones_other_uri_refuses() {
        assert!(KNOWN_SCHEMES
            .iter()
            .all(|k| k.len() <= 4));
        let mut refused: Vec<String> = short_schemes()
            .filter(|s| OtherUri::new(Some(s), "x") == Err(OtherUriError::TypedScheme))
            .collect();
        refused.sort_unstable();
        let mut known: Vec<String> = KNOWN_SCHEMES
            .iter()
            .map(|k| k.to_string())
            .collect();
        known.sort_unstable();
        assert_eq!(refused, known);
    }

    #[test]
    fn find_at_basic() {
        assert_eq!(find_userinfo_at("user@host"), Some(4));
        assert_eq!(find_userinfo_at("host"), None);
        assert_eq!(find_userinfo_at("u@h?From=foo@bar"), Some(1));
    }
}
