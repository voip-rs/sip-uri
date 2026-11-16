use crate::error::ParseError;
use crate::grammar::{self, SchemeSplit};
use crate::warning::{Component, Parsed, WarningCode, Warnings};
use sip_uri_types::{UrnUri, UrnUriParts};

const NID_MAX_LEN: usize = 32;

/// RFC 8141: `NID = (alphanum) 0*30(ldh) (alphanum)` where `ldh = alphanum / "-"`.
fn is_valid_nid(nid: &str) -> bool {
    let bytes = nid.as_bytes();
    (2..=NID_MAX_LEN).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'-')
}

pub(crate) fn parse(input: &str) -> Result<Parsed<UrnUri>, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut warnings = Warnings::new(input);
    let mut parts = UrnUriParts::default();

    let rest = match grammar::split_scheme(input) {
        SchemeSplit::Named(s, rest) if s.eq_ignore_ascii_case("urn") => rest,
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

    let (nid_str, after_nid) = rest.split_at(nid_end(rest));
    let after_nid = after_nid
        .strip_prefix(':')
        .unwrap_or(after_nid);
    if nid_str.is_empty() {
        warnings.push(Component::Nid, WarningCode::MissingNid, nid_str, 0);
    } else {
        if !is_valid_nid(nid_str) {
            warnings.push(Component::Nid, WarningCode::InvalidNid, nid_str, 0);
        }
        parts.nid = Some(nid_str.to_string());
    }

    // `#` appears in no component, so the first one starts the fragment.
    let before_fragment = match after_nid.split_once('#') {
        Some((before, frag)) => {
            warnings.charset(Component::FComponent, frag, grammar::URN_RQF);
            parts.f_component = Some(frag.to_string());
            before
        }
        None => after_nid,
    };

    // `?` is not a pchar: `?+` or `?=` ends the NSS, any other `?` stays in it.
    let rq_start = before_fragment
        .match_indices('?')
        .map(|(i, _)| i)
        .find(|&i| {
            matches!(
                before_fragment
                    .as_bytes()
                    .get(i + 1),
                Some(b'+' | b'=')
            )
        });
    let (nss_str, rq_str) = match rq_start {
        Some(q) => (&before_fragment[..q], Some(&before_fragment[q..])),
        None => (before_fragment, None),
    };

    if nss_str.is_empty() {
        warnings.push(Component::Nss, WarningCode::MissingNss, nss_str, 0);
    } else {
        if nss_str.starts_with('/') {
            warnings.push(Component::Nss, WarningCode::InvalidChar, nss_str, 0);
        } else {
            warnings.charset(Component::Nss, nss_str, grammar::URN_NSS);
        }
        parts.nss = Some(nss_str.to_string());
    }

    let (r_component, q_component) = match rq_str {
        Some(rq) => parse_rq_components(rq),
        None => (None, None),
    };
    for (component, value) in [
        (Component::RComponent, r_component),
        (Component::QComponent, q_component),
    ] {
        if let Some(value) = value {
            if value.is_empty() {
                warnings.push(component, WarningCode::EmptyComponent, value, 0);
            }
            warnings.charset(component, value, grammar::URN_RQF);
        }
    }
    parts.r_component = r_component.map(str::to_string);
    parts.q_component = q_component.map(str::to_string);

    Ok(warnings.finish(UrnUri::from(parts)))
}

/// The NID ends at the `:` before the NSS, or, with the NSS missing, where an
/// r-, q- or f-component starts: `ldh` holds none of `:?#`.
fn nid_end(s: &str) -> usize {
    let bytes = s.as_bytes();
    (0..bytes.len())
        .find(|&i| match bytes[i] {
            b':' | b'#' => true,
            b'?' => matches!(bytes.get(i + 1), Some(b'+' | b'=')),
            _ => false,
        })
        .unwrap_or(bytes.len())
}

/// Split `?+r` and/or `?=q`; the input starts with one of the two.
fn parse_rq_components(s: &str) -> (Option<&str>, Option<&str>) {
    if let Some(r) = s.strip_prefix("?+") {
        match r.split_once("?=") {
            Some((r, q)) => (Some(r), Some(q)),
            None => (Some(r), None),
        }
    } else {
        (None, s.strip_prefix("?="))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UriParse;

    #[test]
    fn parse_service_sos() {
        let urn = UrnUri::parse("urn:service:sos").unwrap();
        assert_eq!(urn.nid(), Some("service"));
        assert_eq!(urn.nss(), Some("sos"));
    }

    #[test]
    fn parse_service_sos_subtype() {
        let urn = UrnUri::parse("urn:service:sos.fire").unwrap();
        assert_eq!(urn.nid(), Some("service"));
        assert_eq!(urn.nss(), Some("sos.fire"));
    }

    #[test]
    fn parse_nena_service() {
        let urn = UrnUri::parse("urn:nena:service:sos").unwrap();
        assert_eq!(urn.nid(), Some("nena"));
        assert_eq!(urn.nss(), Some("service:sos"));
    }

    #[test]
    fn parse_nena_uid_callid() {
        let urn = UrnUri::parse("urn:nena:callid:20250101120000001TEST001:bcf1.ng911.example.com")
            .unwrap();
        assert_eq!(urn.nid(), Some("nena"));
        assert_eq!(
            urn.nss()
                .unwrap(),
            "callid:20250101120000001TEST001:bcf1.ng911.example.com"
        );
    }

    #[test]
    fn parse_emergency_incidentid() {
        let urn = UrnUri::parse(
            "urn:emergency:incidentid:f1e2d3c4b5a6f7e8d9c0b1a2f3e4d5c6:bcf.ng911.example.com",
        )
        .unwrap();
        assert_eq!(urn.nid(), Some("emergency"));
        assert!(urn
            .nss()
            .unwrap()
            .starts_with("incidentid:"));
    }

    #[test]
    fn parse_gsma_imei() {
        let urn = UrnUri::parse("urn:gsma:imei:35625207-210812-0").unwrap();
        assert_eq!(urn.nid(), Some("gsma"));
        assert_eq!(urn.nss(), Some("imei:35625207-210812-0"));
    }

    #[test]
    fn parse_urn7_3gpp() {
        let urn = UrnUri::parse("urn:urn-7:3gpp-service.ims.icsi.mmtel").unwrap();
        assert_eq!(urn.nid(), Some("urn-7"));
        assert_eq!(urn.nss(), Some("3gpp-service.ims.icsi.mmtel"));
    }

    #[test]
    fn parse_uuid() {
        let urn = UrnUri::parse("urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6").unwrap();
        assert_eq!(urn.nid(), Some("uuid"));
        assert_eq!(urn.nss(), Some("f81d4fae-7dec-11d0-a765-00a0c91e6bf6"));
    }

    #[test]
    fn parse_case_insensitive_scheme() {
        let urn = UrnUri::parse("URN:service:sos").unwrap();
        assert_eq!(urn.nid(), Some("service"));
    }

    #[test]
    fn nid_case_insensitive() {
        let urn = UrnUri::parse("urn:SERVICE:sos").unwrap();
        assert_eq!(urn.nid(), Some("service"));
    }

    #[test]
    fn nss_percent_encoding_uppercased() {
        let urn = UrnUri::parse("urn:example:foo%2fbar").unwrap();
        assert_eq!(urn.nss(), Some("foo%2Fbar"));
    }

    #[test]
    fn display_roundtrip() {
        let input = "urn:service:sos.police";
        let urn = UrnUri::parse(input).unwrap();
        assert_eq!(urn.to_string(), input);
    }

    #[test]
    fn display_roundtrip_nena_callid() {
        let input = "urn:nena:callid:abc123:host.example.com";
        let urn = UrnUri::parse(input).unwrap();
        assert_eq!(urn.to_string(), input);
    }

    #[test]
    fn with_rq_components() {
        let urn = UrnUri::parse("urn:example:foo?+resolve?=query#frag").unwrap();
        assert_eq!(urn.nss(), Some("foo"));
        assert_eq!(urn.r_component(), Some("resolve"));
        assert_eq!(urn.q_component(), Some("query"));
        assert_eq!(urn.f_component(), Some("frag"));
        assert_eq!(urn.to_string(), "urn:example:foo?+resolve?=query#frag");
    }

    #[test]
    fn with_r_component_only() {
        let urn = UrnUri::parse("urn:example:foo?+resolve").unwrap();
        assert_eq!(urn.r_component(), Some("resolve"));
        assert_eq!(urn.q_component(), None);
    }

    #[test]
    fn with_q_component_only() {
        let urn = UrnUri::parse("urn:example:foo?=query").unwrap();
        assert_eq!(urn.r_component(), None);
        assert_eq!(urn.q_component(), Some("query"));
    }

    #[test]
    fn with_fragment_only() {
        let urn = UrnUri::parse("urn:example:foo#section1").unwrap();
        assert_eq!(urn.f_component(), Some("section1"));
        assert_eq!(urn.r_component(), None);
        assert_eq!(urn.q_component(), None);
    }

    #[test]
    fn assigned_name() {
        let urn = UrnUri::parse("urn:service:sos?+r?=q#f").unwrap();
        assert_eq!(
            urn.assigned_name()
                .to_string(),
            "urn:service:sos"
        );
    }

    #[test]
    fn nss_with_slashes() {
        let urn = UrnUri::parse("urn:example:a/b/c").unwrap();
        assert_eq!(urn.nss(), Some("a/b/c"));
    }

    #[test]
    fn nss_with_colons() {
        let urn = UrnUri::parse("urn:example:a:b:c").unwrap();
        assert_eq!(urn.nss(), Some("a:b:c"));
    }

    // Negative tests

    #[test]
    fn missing_scheme() {
        assert!(UrnUri::parse("service:sos").is_err());
    }

    #[test]
    fn wrong_scheme() {
        assert!(UrnUri::parse("http:service:sos").is_err());
    }

    #[test]
    fn nonconformant_nid_and_nss_warn() {
        let long_nid = format!("urn:{}:foo", "a".repeat(33));
        for (input, code) in [
            ("urn:x:foo", WarningCode::InvalidNid),
            (long_nid.as_str(), WarningCode::InvalidNid),
            ("urn:-ab:foo", WarningCode::InvalidNid),
            ("urn:ab-:foo", WarningCode::InvalidNid),
            ("urn:example:", WarningCode::MissingNss),
            ("urn:example:/foo", WarningCode::InvalidChar),
            ("urn:example:foo?x", WarningCode::InvalidChar),
        ] {
            let parsed = UrnUri::parse_with_warnings(input).unwrap();
            assert_eq!(
                parsed
                    .warnings
                    .iter()
                    .map(|w| w.code)
                    .collect::<Vec<_>>(),
                [code],
                "{input}"
            );
            assert_eq!(
                UrnUri::parse(
                    &parsed
                        .value
                        .to_string()
                )
                .unwrap(),
                parsed.value,
                "{input}"
            );
        }
    }

    #[test]
    fn missing_nss_ends_the_nid_at_the_next_component() {
        for (input, r, q, f) in [
            ("urn:example?+r", Some("r"), None, None),
            ("urn:example?=q", None, Some("q"), None),
            ("urn:example#f:g", None, None, Some("f:g")),
            ("urn:?+r?=q#f", Some("r"), Some("q"), Some("f")),
        ] {
            let parsed = UrnUri::parse_with_warnings(input).unwrap();
            let urn = &parsed.value;
            assert_eq!(urn.nss(), None, "{input}");
            assert_eq!(
                (urn.r_component(), urn.q_component(), urn.f_component()),
                (r, q, f),
                "{input}"
            );
            assert!(parsed
                .warnings
                .iter()
                .any(|w| w.code == WarningCode::MissingNss));
        }
        assert_eq!(
            UrnUri::parse("urn:example#f")
                .unwrap()
                .nid(),
            Some("example")
        );
        assert_eq!(
            UrnUri::parse("urn:ab?c:d")
                .unwrap()
                .nid(),
            Some("ab%3Fc")
        );
    }

    #[test]
    fn service_sos_with_port_in_to_header() {
        // Seen in production: `<urn:service:sos:5060>` in To header
        // The `:5060` is part of the NSS (not a port), and parses fine
        let urn = UrnUri::parse("urn:service:sos:5060").unwrap();
        assert_eq!(urn.nid(), Some("service"));
        assert_eq!(urn.nss(), Some("sos:5060"));
    }

    #[test]
    fn vendor_provider_id() {
        let urn = UrnUri::parse("urn:example:ng911:lsp:provider1").unwrap();
        assert_eq!(urn.nid(), Some("example"));
        assert_eq!(urn.nss(), Some("ng911:lsp:provider1"));
    }

    #[test]
    fn nena_service_responder_police() {
        let urn = UrnUri::parse("urn:nena:service:responder.police").unwrap();
        assert_eq!(urn.nid(), Some("nena"));
        assert_eq!(urn.nss(), Some("service:responder.police"));
    }
}
