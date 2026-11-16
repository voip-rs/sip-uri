use super::params::{self, parse_params};
use crate::grammar::{self, SchemeSplit};
use crate::warning::{Parsed, Warnings};
use sip_uri_diagnostics::{Component, ParseError, WarningCode};
use sip_uri_types::{TelUri, TelUriParts};

/// RFC 3966: `phonedigit = DIGIT / visual-separator`
/// `visual-separator = "-" / "." / "(" / ")"`
fn is_phonedigit(c: u8) -> bool {
    c.is_ascii_digit() || matches!(c, b'-' | b'.' | b'(' | b')')
}

/// RFC 3966: `phonedigit-hex = HEXDIG / "*" / "#" / visual-separator`
fn is_phonedigit_hex(c: u8) -> bool {
    c.is_ascii_hexdigit() || matches!(c, b'*' | b'#' | b'-' | b'.' | b'(' | b')')
}

pub(crate) fn parse(input: &str) -> Result<Parsed<TelUri>, ParseError> {
    parse_split(input, grammar::split_scheme(input))
}

/// [`parse`] of `input`, whose scheme `split` already read.
pub(crate) fn parse_split(
    input: &str,
    split: SchemeSplit<'_>,
) -> Result<Parsed<TelUri>, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut warnings = Warnings::new(input);
    let mut parts = TelUriParts::default();

    let rest = match split {
        SchemeSplit::Named(s, rest) if s.eq_ignore_ascii_case("tel") => rest,
        SchemeSplit::Named(..) => return Err(ParseError::SchemeMismatch),
        SchemeSplit::Invalid => {
            warnings.push(Component::Scheme, WarningCode::InvalidScheme, input, 0);
            input
        }
        SchemeSplit::Absent => {
            warnings.push(Component::Scheme, WarningCode::MissingScheme, input, 0);
            input
        }
    };

    let (number_str, params_str) = match rest.split_once(';') {
        Some((number, params)) => (number, Some(params)),
        None => (rest, None),
    };

    // `#` is a phonedigit-hex, so a fragment can only follow the params.
    let params_str = match params_str.map(|p| (p, p.find('#'))) {
        Some((p, Some(hash_pos))) => {
            let frag = &p[hash_pos + 1..];
            if frag.is_empty() {
                warnings.push(Component::Fragment, WarningCode::EmptyFragment, p, hash_pos);
            } else {
                warnings.push(
                    Component::Fragment,
                    WarningCode::UnexpectedFragment,
                    p,
                    hash_pos,
                );
                parts.fragment = Some(frag.to_string());
            }
            Some(&p[..hash_pos])
        }
        Some((p, None)) => Some(p),
        None => None,
    };

    if let Some(p) = params_str {
        parse_params(p, &params::TEL_PARAMS, &mut warnings, |n, v| {
            parts
                .params
                .try_push(n, v)
                .is_ok()
        });
    }

    if number_str.is_empty() {
        warnings.push(Component::Number, WarningCode::MissingNumber, number_str, 0);
    } else {
        parts.number = Some(number_str.to_string());
    }

    let tel = TelUri::from(parts);
    if !number_str.is_empty() {
        warn_number(number_str, &tel, &mut warnings);
    }
    Ok(warnings.finish(tel))
}

/// RFC 3966: a local number needs a HEXDIG, `*` or `#`.
fn is_local_digit(b: u8) -> bool {
    b.is_ascii_hexdigit() || matches!(b, b'*' | b'#')
}

fn warn_number(number: &str, tel: &TelUri, warnings: &mut Warnings) {
    let global = number.strip_prefix('+');
    let digits = global.unwrap_or(number);
    type ByteClass = fn(u8) -> bool;
    let (allowed, is_digit): (ByteClass, ByteClass) = if global.is_some() {
        (is_phonedigit, |b| b.is_ascii_digit())
    } else {
        (is_phonedigit_hex, is_local_digit)
    };
    if let Some(pos) = digits
        .bytes()
        .position(|b| !allowed(b))
    {
        warnings.push(Component::Number, WarningCode::InvalidChar, digits, pos);
    }
    if !digits
        .bytes()
        .any(is_digit)
    {
        warnings.push(Component::Number, WarningCode::NoDigits, number, 0);
    }
    if global.is_none()
        && tel
            .param("phone-context")
            .is_none()
    {
        warnings.push(
            Component::Number,
            WarningCode::MissingPhoneContext,
            number,
            0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UriParse;

    #[test]
    fn parse_global() {
        let uri = TelUri::parse("tel:+12345678").unwrap();
        assert_eq!(uri.number(), Some("+12345678"));
        assert!(uri.is_global());
        assert!(uri
            .params()
            .is_empty());
    }

    #[test]
    fn parse_with_params() {
        let uri = TelUri::parse("tel:+12345678;param=1;param=2").unwrap();
        assert_eq!(uri.number(), Some("+12345678"));
        assert_eq!(
            uri.params()
                .len(),
            2
        );
    }

    #[test]
    fn parse_local_number() {
        let uri = TelUri::parse("tel:911").unwrap();
        assert_eq!(uri.number(), Some("911"));
        assert!(!uri.is_global());
    }

    #[test]
    fn parse_local_with_context() {
        let uri = TelUri::parse("tel:1411;phone-context=example.com").unwrap();
        assert_eq!(uri.number(), Some("1411"));
        assert_eq!(uri.param("phone-context"), Some(Some("example.com")));
    }

    #[test]
    fn parse_visual_separators() {
        let uri = TelUri::parse("tel:+1.245.623-57").unwrap();
        assert_eq!(uri.number(), Some("+1.245.623-57"));
    }

    #[test]
    fn parse_dtmf_local() {
        let uri = TelUri::parse("tel:*67").unwrap();
        assert_eq!(uri.number(), Some("*67"));
    }

    #[test]
    fn display_roundtrip() {
        let input = "tel:+12345678;cpc=emergency;oli=0";
        let uri = TelUri::parse(input).unwrap();
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn empty_number_is_none() {
        let parsed = TelUri::parse_with_warnings("tel:").unwrap();
        assert_eq!(
            parsed
                .value
                .number(),
            None
        );
        assert_eq!(parsed.warnings[0].code, WarningCode::MissingNumber);
    }

    #[test]
    fn plus_only_has_no_digits() {
        let parsed = TelUri::parse_with_warnings("tel:+").unwrap();
        assert_eq!(
            parsed
                .value
                .number(),
            Some("+")
        );
        assert_eq!(parsed.warnings[0].code, WarningCode::NoDigits);
    }

    #[test]
    fn empty_input_fails() {
        assert!(TelUri::parse("").is_err());
    }

    #[test]
    fn param_without_value() {
        let uri = TelUri::parse("tel:+12345678;oli").unwrap();
        assert_eq!(uri.param("oli"), Some(None));
    }
}
