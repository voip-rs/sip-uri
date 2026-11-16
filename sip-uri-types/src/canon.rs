//! Canonical form of each URI component, and the literal sets deciding it.

use std::borrow::Cow;

pub mod encoding;

/// RFC 3261 §25: `unreserved = alphanum / mark`
fn is_unreserved(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        )
}

/// Octets a user part writes literally: `unreserved` and `user-unreserved`
/// but `;`, since a literal one starts user-params.
fn is_user_literal(c: u8) -> bool {
    is_unreserved(c) || matches!(c, b'&' | b'=' | b'+' | b'$' | b',' | b'?' | b'/')
}

/// The user-literal set plus `#`, which phones and dialplans use unescaped.
fn is_user_literal_or_hash(c: u8) -> bool {
    is_user_literal(c) || c == b'#'
}

/// Octets a user-param name writes literally, where `=` would start the value.
fn is_user_param_name_literal(c: u8) -> bool {
    is_user_literal(c) && c != b'='
}

/// RFC 3261 §25 password characters.
fn is_password_literal(c: u8) -> bool {
    is_unreserved(c) || matches!(c, b'&' | b'=' | b'+' | b'$' | b',')
}

/// RFC 3261 §25: `unreserved / param-unreserved`.
fn is_param_literal(c: u8) -> bool {
    is_unreserved(c) || matches!(c, b'[' | b']' | b'/' | b':' | b'&' | b'+' | b'$')
}

/// RFC 3261 §25: `unreserved / hnv-unreserved`.
fn is_hnv_literal(c: u8) -> bool {
    is_unreserved(c) || matches!(c, b'[' | b']' | b'/' | b'?' | b':' | b'+' | b'$')
}

/// RFC 3986 §3.3: `pchar = unreserved / pct-encoded / sub-delims / ":" / "@"`
fn is_pchar(c: u8) -> bool {
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

/// RFC 3986 §3.5 `fragment` characters, plus `#`.
fn is_fragment_literal(c: u8) -> bool {
    is_pchar(c) || matches!(c, b'/' | b'?' | b'#')
}

/// A SIP fragment holds `@` escaped for the reason a param does.
fn is_sip_fragment_literal(c: u8) -> bool {
    is_fragment_literal(c) && c != b'@'
}

/// RFC 8141: `NSS = pchar *(pchar / "/")`.
fn is_nss_literal(c: u8) -> bool {
    is_pchar(c) || c == b'/'
}

/// RFC 8141: r-, q- and f-components are `*( pchar / "/" / "?" )`.
fn is_rqf_literal(c: u8) -> bool {
    is_pchar(c) || matches!(c, b'/' | b'?')
}

/// RFC 8141: `ldh = alphanum / "-"`.
fn is_ldh(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-'
}

fn never(_: u8) -> bool {
    false
}

/// Schemes with their own URI type, never held by an `OtherUri`.
const KNOWN_SCHEMES: [&str; 4] = ["sip", "sips", "tel", "urn"];

pub(crate) fn is_known_scheme(s: &str) -> bool {
    KNOWN_SCHEMES
        .iter()
        .any(|k| s.eq_ignore_ascii_case(k))
}

/// RFC 3986 §3.1: `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
pub(crate) fn is_scheme(s: &str) -> bool {
    let mut bytes = s.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

const ESCAPE_LEN: usize = 3;
const HEX_RADIX: u32 = 16;
const NIBBLE_BITS: u32 = 4;

/// The octet escaped by a well-formed `%XX` starting at `i`.
fn escape_at(bytes: &[u8], i: usize) -> Option<u8> {
    if bytes.get(i) != Some(&b'%') {
        return None;
    }
    let hi = char::from(*bytes.get(i + 1)?).to_digit(HEX_RADIX)?;
    let lo = char::from(*bytes.get(i + 2)?).to_digit(HEX_RADIX)?;
    Some(((hi << NIBBLE_BITS) | lo) as u8)
}

/// Canonize one component: an escape of an octet in `decode` and a literal
/// octet in `keep` are written literally, every other octet as uppercase `%XX`.
///
/// `decode` must be a subset of `keep`, which then makes the result
/// idempotent. A `%` not starting a valid escape is the octet `%`.
fn canonize(input: &str, decode: fn(u8) -> bool, keep: fn(u8) -> bool) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match escape_at(bytes, i) {
            Some(b) => {
                push_octet(&mut out, b, decode);
                i += ESCAPE_LEN;
            }
            None => {
                push_octet(&mut out, bytes[i], keep);
                i += 1;
            }
        }
    }
    out
}

/// [`canonize`], borrowing `input` when it is already canonical: no escape,
/// and every byte one `keep` holds literal.
fn canonize_cow(input: &str, decode: fn(u8) -> bool, keep: fn(u8) -> bool) -> Cow<'_, str> {
    if input
        .bytes()
        .all(|b| b != b'%' && b.is_ascii() && keep(b))
    {
        Cow::Borrowed(input)
    } else {
        Cow::Owned(canonize(input, decode, keep))
    }
}

fn push_octet(out: &mut String, b: u8, literal: fn(u8) -> bool) {
    if b.is_ascii() && literal(b) {
        out.push(b as char);
    } else {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        out.push('%');
        out.push(HEX[(b >> NIBBLE_BITS) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
}

/// Lowercase every literal octet, leaving escape hex uppercase.
fn lowercase_literals(mut s: String) -> String {
    let mut i = 0;
    while i < s.len() {
        if s.as_bytes()[i] == b'%' {
            i += ESCAPE_LEN;
        } else {
            if let Some(octet) = s.get_mut(i..i + 1) {
                octet.make_ascii_lowercase();
            }
            i += 1;
        }
    }
    s
}

/// Canonize a SIP user part or user-param value.
pub(crate) fn canonize_user(input: &str) -> String {
    canonize(input, is_unreserved, is_user_literal_or_hash)
}

/// Canonize a user-param name and value.
pub(crate) fn canonize_user_param(name: &str, value: Option<&str>) -> (String, Option<String>) {
    (
        canonize(name, is_unreserved, is_user_param_name_literal),
        value.map(canonize_user),
    )
}

/// Canonize a password.
pub(crate) fn canonize_password(input: &str) -> String {
    canonize(input, is_unreserved, is_password_literal)
}

/// Canonize a SIP or tel: param name and value.
pub(crate) fn canonize_param(name: &str, value: Option<&str>) -> (String, Option<String>) {
    let one = |s: &str| canonize(s, is_unreserved, is_param_literal);
    (one(name), value.map(one))
}

/// Canonize a SIP URI header name and value.
pub(crate) fn canonize_header(name: &str, value: Option<&str>) -> (String, Option<String>) {
    let one = |s: &str| canonize(s, is_unreserved, is_hnv_literal);
    (one(name), value.map(one))
}

/// A user-param name as [`canonize_user_param`] holds it, borrowed when
/// unchanged.
pub(crate) fn user_param_name(name: &str) -> Cow<'_, str> {
    canonize_cow(name, is_unreserved, is_user_param_name_literal)
}

/// A param name as [`canonize_param`] holds it, borrowed when unchanged.
pub(crate) fn param_name(name: &str) -> Cow<'_, str> {
    canonize_cow(name, is_unreserved, is_param_literal)
}

/// A header name as [`canonize_header`] holds it, borrowed when unchanged.
pub(crate) fn header_name(name: &str) -> Cow<'_, str> {
    canonize_cow(name, is_unreserved, is_hnv_literal)
}

/// Canonize a tel: number.
pub(crate) fn canonize_tel_number(input: &str) -> String {
    canonize(input, never, is_user_literal_or_hash)
}

/// Canonize a SIP fragment.
pub(crate) fn canonize_sip_fragment(input: &str) -> String {
    canonize(input, never, is_sip_fragment_literal)
}

/// Canonize a tel: fragment or URN f-component.
pub(crate) fn canonize_fragment(input: &str) -> String {
    canonize(input, never, is_fragment_literal)
}

/// Canonize a URN NSS.
pub(crate) fn canonize_nss(input: &str) -> String {
    canonize(input, never, is_nss_literal)
}

/// Canonize a URN r-component.
pub(crate) fn canonize_urn_r(input: &str) -> String {
    escape_q_start(canonize(input, never, is_rqf_literal))
}

/// Escape a `?` before `=` in an r-component, since `?=` would start the
/// q-component.
fn escape_q_start(r: String) -> String {
    r.replace("?=", "%3F=")
}

/// Canonize a URN q-component.
pub(crate) fn canonize_urn_q(input: &str) -> String {
    canonize(input, never, is_rqf_literal)
}

/// Canonize a URN NID, lowercase.
pub(crate) fn canonize_nid(input: &str) -> String {
    lowercase_literals(canonize(input, never, is_ldh))
}

/// Canonize a hostname, lowercase with uppercase escape hex.
pub(crate) fn canonize_hostname(input: &str) -> String {
    lowercase_literals(canonize(input, is_unreserved, is_unreserved))
}

/// Octets the text of an unrecognized URI writes literally: any ASCII but
/// controls, space and the `<` `>` `"` that delimit a URI in a header.
fn is_other_literal(c: u8) -> bool {
    !c.is_ascii_control() && !matches!(c, b' ' | b'<' | b'>' | b'"')
}

/// Canonize the text of an unrecognized URI, decoding nothing.
pub(crate) fn canonize_other(input: &str) -> String {
    escape(input.as_bytes(), is_other_literal)
}

/// Write each octet in `literal` as itself and every other as uppercase `%XX`.
fn escape(bytes: &[u8], literal: fn(u8) -> bool) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        push_octet(&mut out, b, literal);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonizers_are_idempotent() {
        let fns: [fn(&str) -> String; 14] = [
            canonize_user,
            |s| canonize_user_param(s, None).0,
            canonize_password,
            |s| canonize_param(s, None).0,
            |s| canonize_header(s, None).0,
            canonize_tel_number,
            canonize_sip_fragment,
            canonize_fragment,
            canonize_nss,
            canonize_urn_r,
            canonize_urn_q,
            canonize_nid,
            canonize_hostname,
            canonize_other,
        ];
        let input = "aZ%41%3b;=?@#% %zz%e2%9c%93é/:&+$,[]!~*'()";
        for f in fns {
            let once = f(input);
            assert!(once.is_ascii(), "{once}");
            assert_eq!(f(&once), once);
        }
    }

    #[test]
    fn canonize_sip_user() {
        assert_eq!(canonize_user("pekka%2Epessi"), "pekka.pessi");
        assert_eq!(canonize_user("%22foo%22"), "%22foo%22");
        assert_eq!(canonize_user("%2e%23#"), ".%23#");
    }
}
