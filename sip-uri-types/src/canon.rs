//! Canonical form of each URI component, and the literal sets deciding it.
//!
//! The `is_*` predicates are the source of truth for each literal set. The
//! canonizers read them through `CLASSES`, a byte table built from them at
//! compile time with one bit per set, so a byte's membership is one load
//! instead of an indirect call. A new set is a predicate first, then a bit
//! and a row in the table and in its test, which checks every byte of every
//! set against its predicate.

use std::borrow::Cow;

pub mod encoding;

/// RFC 3261 §25: `unreserved = alphanum / mark`
const fn is_unreserved(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
        )
}

/// Octets a user part writes literally: `unreserved` and `user-unreserved`
/// but `;`, since a literal one starts user-params.
const fn is_user_literal(c: u8) -> bool {
    is_unreserved(c) || matches!(c, b'&' | b'=' | b'+' | b'$' | b',' | b'?' | b'/')
}

/// The user-literal set plus `#`, which phones and dialplans use unescaped.
const fn is_user_literal_or_hash(c: u8) -> bool {
    is_user_literal(c) || c == b'#'
}

/// Octets a user-param name writes literally, where `=` would start the value.
const fn is_user_param_name_literal(c: u8) -> bool {
    is_user_literal(c) && c != b'='
}

/// RFC 3261 §25 password characters.
const fn is_password_literal(c: u8) -> bool {
    is_unreserved(c) || matches!(c, b'&' | b'=' | b'+' | b'$' | b',')
}

/// RFC 3261 §25: `unreserved / param-unreserved`.
const fn is_param_literal(c: u8) -> bool {
    is_unreserved(c) || matches!(c, b'[' | b']' | b'/' | b':' | b'&' | b'+' | b'$')
}

/// RFC 3261 §25: `unreserved / hnv-unreserved`.
const fn is_hnv_literal(c: u8) -> bool {
    is_unreserved(c) || matches!(c, b'[' | b']' | b'/' | b'?' | b':' | b'+' | b'$')
}

/// RFC 3986 §3.3: `pchar = unreserved / pct-encoded / sub-delims / ":" / "@"`
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

/// RFC 3986 §3.5 `fragment` characters, plus `#`.
const fn is_fragment_literal(c: u8) -> bool {
    is_pchar(c) || matches!(c, b'/' | b'?' | b'#')
}

/// A SIP fragment holds `@` escaped for the reason a param does.
const fn is_sip_fragment_literal(c: u8) -> bool {
    is_fragment_literal(c) && c != b'@'
}

/// RFC 8141: `NSS = pchar *(pchar / "/")`.
const fn is_nss_literal(c: u8) -> bool {
    is_pchar(c) || c == b'/'
}

/// RFC 8141: r-, q- and f-components are `*( pchar / "/" / "?" )`.
const fn is_rqf_literal(c: u8) -> bool {
    is_pchar(c) || matches!(c, b'/' | b'?')
}

/// RFC 8141: `ldh = alphanum / "-"`.
const fn is_ldh(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-'
}

/// A set of literal octets, one bit of [`CLASSES`] each.
#[derive(Clone, Copy)]
struct Class(u16);

const NEVER: Class = Class(0);
const UNRESERVED: Class = Class(1 << 0);
const USER_OR_HASH: Class = Class(1 << 1);
const USER_PARAM_NAME: Class = Class(1 << 2);
const PASSWORD: Class = Class(1 << 3);
const PARAM: Class = Class(1 << 4);
const HNV: Class = Class(1 << 5);
const PCHAR: Class = Class(1 << 6);
const FRAGMENT: Class = Class(1 << 7);
const SIP_FRAGMENT: Class = Class(1 << 8);
const NSS: Class = Class(1 << 9);
const RQF: Class = Class(1 << 10);
const LDH: Class = Class(1 << 11);
const OTHER: Class = Class(1 << 12);

const fn bit(class: Class, member: bool) -> u16 {
    if member {
        class.0
    } else {
        0
    }
}

/// The classes each octet belongs to, built from the predicates. Non-ASCII
/// octets belong to none, so they are always escaped.
const CLASSES: [u16; 256] = {
    let mut table = [0; 256];
    let mut c = 0;
    while c < 0x80 {
        table[c as usize] = bit(UNRESERVED, is_unreserved(c))
            | bit(USER_OR_HASH, is_user_literal_or_hash(c))
            | bit(USER_PARAM_NAME, is_user_param_name_literal(c))
            | bit(PASSWORD, is_password_literal(c))
            | bit(PARAM, is_param_literal(c))
            | bit(HNV, is_hnv_literal(c))
            | bit(PCHAR, is_pchar(c))
            | bit(FRAGMENT, is_fragment_literal(c))
            | bit(SIP_FRAGMENT, is_sip_fragment_literal(c))
            | bit(NSS, is_nss_literal(c))
            | bit(RQF, is_rqf_literal(c))
            | bit(LDH, is_ldh(c))
            | bit(OTHER, is_other_literal(c));
        c += 1;
    }
    table
};

impl Class {
    fn holds(self, b: u8) -> bool {
        CLASSES[b as usize] & self.0 != 0
    }
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
/// idempotent. A `%` not starting a valid escape is the octet `%`. An owned
/// `input` already canonical is returned as it is, without a copy.
fn canonize(input: Cow<'_, str>, decode: Class, keep: Class) -> String {
    match canonical_len(&input, keep) {
        len if len == input.len() => input.into_owned(),
        start => canonize_from(&input, start, decode, keep),
    }
}

/// The length of the prefix of `input` already canonical: no `%`, and
/// every byte one `keep` holds literal, so ASCII and a char boundary.
fn canonical_len(input: &str, keep: Class) -> usize {
    input
        .bytes()
        .position(|b| b == b'%' || !keep.holds(b))
        .unwrap_or(input.len())
}

/// [`canonize`] of `input` whose first `start` bytes are canonical.
fn canonize_from(input: &str, start: usize, decode: Class, keep: Class) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    out.push_str(&input[..start]);
    let mut i = start;
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

/// [`canonize`], borrowing `input` when it is already canonical.
fn canonize_cow(input: &str, decode: Class, keep: Class) -> Cow<'_, str> {
    match canonical_len(input, keep) {
        len if len == input.len() => Cow::Borrowed(input),
        start => Cow::Owned(canonize_from(input, start, decode, keep)),
    }
}

fn push_octet(out: &mut String, b: u8, literal: Class) {
    if literal.holds(b) {
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
    if !s
        .bytes()
        .any(|b| b.is_ascii_uppercase())
    {
        return s;
    }
    if !s.contains('%') {
        s.make_ascii_lowercase();
        return s;
    }
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
pub(crate) fn canonize_user<'a>(input: impl Into<Cow<'a, str>>) -> String {
    canonize(input.into(), UNRESERVED, USER_OR_HASH)
}

/// Canonize a user-param name and value.
pub(crate) fn canonize_user_param(name: &str, value: Option<&str>) -> (String, Option<String>) {
    (
        canonize(name.into(), UNRESERVED, USER_PARAM_NAME),
        value.map(canonize_user),
    )
}

/// Canonize a password.
pub(crate) fn canonize_password<'a>(input: impl Into<Cow<'a, str>>) -> String {
    canonize(input.into(), UNRESERVED, PASSWORD)
}

/// Canonize a SIP or tel: param name and value.
pub(crate) fn canonize_param(name: &str, value: Option<&str>) -> (String, Option<String>) {
    let one = |s: &str| canonize(s.into(), UNRESERVED, PARAM);
    (one(name), value.map(one))
}

/// Canonize a SIP URI header name and value.
pub(crate) fn canonize_header(name: &str, value: Option<&str>) -> (String, Option<String>) {
    let one = |s: &str| canonize(s.into(), UNRESERVED, HNV);
    (one(name), value.map(one))
}

/// A user-param name as [`canonize_user_param`] holds it, borrowed when
/// unchanged.
pub(crate) fn user_param_name(name: &str) -> Cow<'_, str> {
    canonize_cow(name, UNRESERVED, USER_PARAM_NAME)
}

/// A param name as [`canonize_param`] holds it, borrowed when unchanged.
pub(crate) fn param_name(name: &str) -> Cow<'_, str> {
    canonize_cow(name, UNRESERVED, PARAM)
}

/// A header name as [`canonize_header`] holds it, borrowed when unchanged.
pub(crate) fn header_name(name: &str) -> Cow<'_, str> {
    canonize_cow(name, UNRESERVED, HNV)
}

/// Canonize a tel: number.
pub(crate) fn canonize_tel_number<'a>(input: impl Into<Cow<'a, str>>) -> String {
    canonize(input.into(), NEVER, USER_OR_HASH)
}

/// Canonize a SIP fragment.
pub(crate) fn canonize_sip_fragment<'a>(input: impl Into<Cow<'a, str>>) -> String {
    canonize(input.into(), NEVER, SIP_FRAGMENT)
}

/// Canonize a tel: fragment or URN f-component.
pub(crate) fn canonize_fragment<'a>(input: impl Into<Cow<'a, str>>) -> String {
    canonize(input.into(), NEVER, FRAGMENT)
}

/// Canonize a URN NSS.
pub(crate) fn canonize_nss<'a>(input: impl Into<Cow<'a, str>>) -> String {
    canonize(input.into(), NEVER, NSS)
}

/// Canonize a URN r-component.
pub(crate) fn canonize_urn_r<'a>(input: impl Into<Cow<'a, str>>) -> String {
    escape_q_start(canonize(input.into(), NEVER, RQF))
}

/// Escape a `?` before `=` in an r-component, since `?=` would start the
/// q-component.
fn escape_q_start(r: String) -> String {
    if !r.contains("?=") {
        return r;
    }
    r.replace("?=", "%3F=")
}

/// Canonize a URN q-component.
pub(crate) fn canonize_urn_q<'a>(input: impl Into<Cow<'a, str>>) -> String {
    canonize(input.into(), NEVER, RQF)
}

/// Canonize a URN NID, lowercase.
pub(crate) fn canonize_nid<'a>(input: impl Into<Cow<'a, str>>) -> String {
    lowercase_literals(canonize(input.into(), NEVER, LDH))
}

/// Canonize a hostname, lowercase with uppercase escape hex.
pub(crate) fn canonize_hostname<'a>(input: impl Into<Cow<'a, str>>) -> String {
    lowercase_literals(canonize(input.into(), UNRESERVED, UNRESERVED))
}

/// Octets the text of an unrecognized URI writes literally: any ASCII but
/// controls, space and the `<` `>` `"` that delimit a URI in a header.
const fn is_other_literal(c: u8) -> bool {
    !c.is_ascii_control() && !matches!(c, b' ' | b'<' | b'>' | b'"')
}

/// Canonize the text of an unrecognized URI, decoding nothing.
pub(crate) fn canonize_other(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    push_other(&mut out, input);
    out
}

/// Append [`canonize_other`] of `input` to `out`, each run of literal
/// octets copied whole.
pub(crate) fn push_other(out: &mut String, input: &str) {
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // A literal run is ASCII, so both its ends are char boundaries.
        let run_end = bytes[i..]
            .iter()
            .position(|&b| !OTHER.holds(b))
            .map_or(bytes.len(), |n| i + n);
        out.push_str(&input[i..run_end]);
        i = run_end;
        while i < bytes.len() && !OTHER.holds(bytes[i]) {
            push_octet(out, bytes[i], OTHER);
            i += 1;
        }
    }
}

/// Write each octet in `literal` as itself and every other as uppercase `%XX`.
fn escape(bytes: &[u8], literal: Class) -> String {
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
            |s| canonize_user(s),
            |s| canonize_user_param(s, None).0,
            |s| canonize_password(s),
            |s| canonize_param(s, None).0,
            |s| canonize_header(s, None).0,
            |s| canonize_tel_number(s),
            |s| canonize_sip_fragment(s),
            |s| canonize_fragment(s),
            |s| canonize_nss(s),
            |s| canonize_urn_r(s),
            |s| canonize_urn_q(s),
            |s| canonize_nid(s),
            |s| canonize_hostname(s),
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
    fn class_table_matches_predicates() {
        type Predicate = fn(u8) -> bool;
        let classes: [(Class, Predicate); 13] = [
            (UNRESERVED, is_unreserved),
            (USER_OR_HASH, is_user_literal_or_hash),
            (USER_PARAM_NAME, is_user_param_name_literal),
            (PASSWORD, is_password_literal),
            (PARAM, is_param_literal),
            (HNV, is_hnv_literal),
            (PCHAR, is_pchar),
            (FRAGMENT, is_fragment_literal),
            (SIP_FRAGMENT, is_sip_fragment_literal),
            (NSS, is_nss_literal),
            (RQF, is_rqf_literal),
            (LDH, is_ldh),
            (OTHER, is_other_literal),
        ];
        for (class, predicate) in classes {
            for b in 0..=u8::MAX {
                assert_eq!(class.holds(b), b.is_ascii() && predicate(b), "{b:#04x}");
            }
        }
        assert!((0..=u8::MAX).all(|b| !NEVER.holds(b)));
    }

    #[test]
    fn owned_canonical_input_keeps_its_buffer() {
        let user = String::from("alice");
        let buffer = user.as_ptr();
        let held = canonize_user(user);
        assert_eq!(held.as_ptr(), buffer);
        let host = String::from("example.com");
        let buffer = host.as_ptr();
        let held = canonize_hostname(host);
        assert_eq!(held.as_ptr(), buffer);
    }

    #[test]
    fn canonize_sip_user() {
        assert_eq!(canonize_user("pekka%2Epessi"), "pekka.pessi");
        assert_eq!(canonize_user("%22foo%22"), "%22foo%22");
        assert_eq!(canonize_user("%2e%23#"), ".%23#");
    }
}
