//! Parsing into the `Uri` sum type: the scheme picks the typed parser, and
//! anything else is `Other`.
//!
//! Text without a scheme is `Other` with a warning, never a guessed SIP URI,
//! even when it reads as `user@host`: only the caller knows its context says
//! SIP, and it re-parses as `SipUri` for that reading.

use crate::error::ParseError;
use crate::grammar::{self, SchemeSplit};
use crate::warning::{Component, Parsed, WarningCode, Warnings};
use sip_uri_types::{OtherUri, OtherUriError, Uri};

pub(crate) fn parse(s: &str) -> Result<Parsed<Uri>, ParseError> {
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut warnings = Warnings::new(s);
    let other = match grammar::split_scheme(s) {
        SchemeSplit::Named(scheme, _) if scheme.eq_ignore_ascii_case("tel") => {
            return Ok(super::tel::parse(s)?.map(Uri::Tel));
        }
        SchemeSplit::Named(scheme, _)
            if scheme.eq_ignore_ascii_case("sip") || scheme.eq_ignore_ascii_case("sips") =>
        {
            return Ok(super::sip::parse(s)?.map(Uri::Sip));
        }
        SchemeSplit::Named(scheme, _) if scheme.eq_ignore_ascii_case("urn") => {
            return Ok(super::urn::parse(s)?.map(Uri::Urn));
        }
        SchemeSplit::Named(scheme, rest) => {
            warnings.charset(Component::Other, rest, grammar::is_rfc3986_char);
            OtherUri::new(Some(scheme), rest)
        }
        SchemeSplit::Invalid => {
            warnings.push(Component::Scheme, WarningCode::InvalidScheme, s, 0);
            OtherUri::new(None, s)
        }
        SchemeSplit::Absent if s == "*" => {
            warnings.push(Component::Scheme, WarningCode::Wildcard, s, 0);
            OtherUri::new(None, s)
        }
        SchemeSplit::Absent => {
            warnings.push(Component::Scheme, WarningCode::MissingScheme, s, 0);
            OtherUri::new(None, s)
        }
    };
    // `s` is not empty, and `split_scheme` names only RFC 3986 schemes,
    // the typed ones dispatched above.
    let other = other.map_err(|e| match e {
        OtherUriError::Empty => ParseError::Empty,
        _ => ParseError::SchemeMismatch,
    })?;
    Ok(warnings.finish(Uri::Other(other)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UriParse;
    use sip_uri_types::SipUri;
    use sip_uri_types::UrnUri;

    #[test]
    fn scheme_less_text_ending_a_port_at_an_escaped_byte_round_trips() {
        let uri = Uri::parse("l:1>\"").unwrap();
        assert_eq!(Uri::parse(&uri.to_string()).unwrap(), uri);
    }

    #[test]
    fn service_urn_with_port_keeps_its_nss_verbatim() {
        let uri = Uri::parse("urn:service:sos:5060").unwrap();
        let urn = uri
            .as_urn()
            .unwrap();
        assert_eq!(urn.nid(), Some("service"));
        assert_eq!(urn.nss(), Some("sos:5060"));
        assert_eq!(Uri::parse(&uri.to_string()).unwrap(), uri);

        let bracketed = Uri::parse("<urn:service:sos:5060>").unwrap();
        assert_eq!(Uri::parse(&bracketed.to_string()).unwrap(), bracketed);
    }

    #[test]
    fn dispatch_sip() {
        let uri = Uri::parse("sip:alice@example.com").unwrap();
        assert!(uri
            .as_sip()
            .is_some());
        assert!(uri
            .as_tel()
            .is_none());
        assert!(uri
            .as_urn()
            .is_none());
    }

    #[test]
    fn dispatch_sips() {
        let uri = Uri::parse("sips:bob@secure.example.com").unwrap();
        assert!(uri
            .as_sip()
            .is_some());
    }

    #[test]
    fn dispatch_tel() {
        let uri = Uri::parse("tel:+15551234567").unwrap();
        assert!(uri
            .as_tel()
            .is_some());
        assert!(uri
            .as_sip()
            .is_none());
    }

    #[test]
    fn dispatch_urn() {
        let uri = Uri::parse("urn:service:sos").unwrap();
        assert!(uri
            .as_urn()
            .is_some());
        assert!(uri
            .as_sip()
            .is_none());
        assert!(uri
            .as_tel()
            .is_none());
    }

    #[test]
    fn unknown_scheme_stored_as_other() {
        let uri = Uri::parse("http://example.com").unwrap();
        assert_eq!(
            uri.as_other(),
            OtherUri::new(Some("http"), "//example.com")
                .ok()
                .as_ref()
        );
        assert_eq!(uri.scheme(), Some("http"));
        assert!(uri
            .as_sip()
            .is_none());
        assert!(uri
            .as_tel()
            .is_none());
        assert!(uri
            .as_urn()
            .is_none());
        assert_eq!(
            uri.into_other()
                .map(|o| o.to_string()),
            Some("http://example.com".to_string())
        );
    }

    #[test]
    fn other_display_roundtrip() {
        let input = "https://example.com/photo.jpg";
        let uri = Uri::parse(input).unwrap();
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn missing_scheme_is_other() {
        let parsed = Uri::parse_with_warnings("no-colon-here").unwrap();
        assert_eq!(
            parsed
                .value
                .as_other()
                .map(OtherUri::as_str),
            Some("no-colon-here")
        );
        assert_eq!(
            parsed
                .value
                .scheme(),
            None
        );
        assert_eq!(parsed.warnings[0].code, WarningCode::MissingScheme);
        assert!(Uri::parse("").is_err());
    }

    #[test]
    fn display_roundtrip() {
        let input = "sip:alice@example.com;transport=tcp";
        let uri = Uri::parse(input).unwrap();
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn display_roundtrip_urn() {
        let input = "urn:service:sos";
        let uri = Uri::parse(input).unwrap();
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn from_sip_uri() {
        let sip = SipUri::parse("sip:alice@example.com").unwrap();
        let uri: Uri = sip.into();
        assert!(uri
            .as_sip()
            .is_some());
    }

    #[test]
    fn from_urn_uri() {
        let urn = UrnUri::parse("urn:service:sos").unwrap();
        let uri: Uri = urn.into();
        assert!(uri
            .as_urn()
            .is_some());
    }

    #[test]
    fn user_sip() {
        let uri = Uri::parse("sip:alice@example.com").unwrap();
        assert_eq!(uri.user(), Some("alice"));
    }

    #[test]
    fn user_sip_no_user() {
        let uri = Uri::parse("sip:example.com").unwrap();
        assert_eq!(uri.user(), None);
    }

    #[test]
    fn user_tel() {
        let uri = Uri::parse("tel:+15551234567").unwrap();
        assert_eq!(uri.user(), Some("+15551234567"));
    }

    #[test]
    fn user_urn() {
        let uri = Uri::parse("urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6").unwrap();
        assert_eq!(uri.user(), None);
    }

    #[test]
    fn user_other() {
        let uri = Uri::parse("http://example.com").unwrap();
        assert_eq!(uri.user(), None);
    }
}
