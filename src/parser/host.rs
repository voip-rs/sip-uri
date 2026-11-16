use std::net::Ipv6Addr;

use crate::error::ParseError;
use crate::grammar::ESCAPE_LEN;
use crate::warning::{Component, Parsed, WarningCode, Warnings};
use sip_uri_types::Host;

/// Parse a host from a URI string fragment.
///
/// Handles `[IPv6]`, dotted-decimal IPv4, and DNS hostnames.
/// Returns the host, `None` when absent or unreadable, and the bytes consumed.
pub(crate) fn parse_from_uri(s: &str, warnings: &mut Warnings) -> (Option<Host>, usize) {
    if let Some(inner) = s.strip_prefix('[') {
        // IPv6reference = "[" IPv6address "]"
        let Some(end) = inner.find(']') else {
            warnings.push(Component::Host, WarningCode::InvalidIpv6, s, 0);
            return (
                None,
                s.find([';', '?', '#'])
                    .unwrap_or(s.len()),
            );
        };
        let addr = inner[..end]
            .parse::<Ipv6Addr>()
            .ok()
            .or_else(|| zoned_ipv6(&inner[..end], warnings));
        if addr.is_none() {
            warnings.push(Component::Host, WarningCode::InvalidIpv6, s, 0);
        }
        return (addr.map(Host::Ipv6), end + 2);
    }

    {
        let end = find_host_end(s);
        let host_str = &s[..end];

        if host_str.is_empty() {
            warnings.push(Component::Host, WarningCode::MissingHost, s, 0);
            return (None, 0);
        }

        let escaped = host_str.contains('%');
        if escaped {
            warnings.push(Component::Host, WarningCode::EscapedHost, host_str, 0);
        }
        let host = Host::from_name(host_str);
        if let Host::Hostname(name) = &host {
            let checked = if escaped { name.as_str() } else { host_str };
            match checked
                .bytes()
                .position(|b| !(b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.')))
            {
                Some(pos) if checked.len() == host_str.len() => {
                    warnings.push(Component::Host, WarningCode::InvalidChar, host_str, pos)
                }
                Some(_) => warnings.push(Component::Host, WarningCode::InvalidChar, host_str, 0),
                None => warn_hostname_labels(checked, host_str, warnings),
            }
        }
        (Some(host), end)
    }
}

/// The address of `text` written with a zone (`fe80::1%eth0`, or RFC 6874's
/// `fe80::1%25eth0` in brackets), the zone dropped with a warning: a host
/// holds no zone, and reading `fe80` as a hostname names another host.
fn zoned_ipv6(text: &str, warnings: &mut Warnings) -> Option<Ipv6Addr> {
    let percent = text.find('%')?;
    let addr = text[..percent]
        .parse::<Ipv6Addr>()
        .ok()?;
    warnings.push(Component::Host, WarningCode::Ipv6Zone, text, percent);
    Some(addr)
}

/// RFC 3261 §25: `hostname =*( domainlabel "." ) toplabel [ "." ]`, a label
/// neither empty nor hyphen-bounded, the toplabel starting with ALPHA.
///
/// `raw` is the host as written; positions point at its start when it was
/// escaped, since `name` offsets then no longer map onto the input.
fn warn_hostname_labels(name: &str, raw: &str, warnings: &mut Warnings) {
    let labels = name
        .strip_suffix('.')
        .unwrap_or(name);
    let offset = |label: &str| {
        if name.len() == raw.len() {
            label.as_ptr() as usize - name.as_ptr() as usize
        } else {
            0
        }
    };
    let mut last = None;
    for label in labels.split('.') {
        if label.is_empty() || label.starts_with('-') || label.ends_with('-') {
            warnings.push(
                Component::Host,
                WarningCode::InvalidHostLabel,
                raw,
                offset(label),
            );
            return;
        }
        last = Some(label);
    }
    if let Some(top) = last {
        if !top.as_bytes()[0].is_ascii_alphabetic() {
            warnings.push(
                Component::Host,
                WarningCode::NumericToplabel,
                raw,
                offset(top),
            );
        }
    }
}

/// Find the end of a hostname in a URI string, handling percent-encoded sequences.
fn find_host_end(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b':' | b';' | b'?' | b'#' | b'>' => return i,
            b'%' if i + 2 < bytes.len() => {
                // Skip percent-encoded sequence
                i += ESCAPE_LEN;
            }
            _ => i += 1,
        }
    }
    i
}

/// Parse a complete host, with no surrounding URI.
pub(crate) fn parse(s: &str) -> Result<Parsed<Host>, ParseError> {
    if s.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut warnings = Warnings::new(s);
    // A bare IPv6 has to be recognized up front: the URI parser stops the
    // host at the first `:`, which is inside the address here.
    if !s.starts_with('[') {
        let addr = s
            .parse::<Ipv6Addr>()
            .ok()
            .or_else(|| zoned_ipv6(s, &mut warnings));
        if let Some(addr) = addr {
            return Ok(warnings.finish(Host::Ipv6(addr)));
        }
    }

    let (host, consumed) = parse_from_uri(s, &mut warnings);
    let Some(host) = host else {
        return Err(ParseError::NoHost);
    };
    if consumed != s.len() {
        warnings.push(Component::Host, WarningCode::TrailingContent, s, consumed);
    }
    Ok(warnings.finish(host))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UriParse;
    use std::net::Ipv4Addr;

    fn from_uri(s: &str) -> Option<(Host, usize)> {
        let (host, consumed) = parse_from_uri(s, &mut Warnings::new(s));
        host.map(|h| (h, consumed))
    }

    #[test]
    fn parse_ipv4() {
        let (host, consumed) = from_uri("198.51.100.55:5060").unwrap();
        assert_eq!(host, Host::Ipv4(Ipv4Addr::new(198, 51, 100, 55)));
        assert_eq!(consumed, 13);
    }

    #[test]
    fn parse_ipv6() {
        let (host, consumed) = from_uri("[::1]:56001").unwrap();
        assert_eq!(host, Host::Ipv6(Ipv6Addr::LOCALHOST));
        assert_eq!(consumed, 5);
    }

    #[test]
    fn parse_ipv6_full() {
        let (host, consumed) = from_uri("[2001:db8::1]:5061").unwrap();
        assert_eq!(
            host,
            Host::Ipv6(
                "2001:db8::1"
                    .parse::<Ipv6Addr>()
                    .unwrap()
            )
        );
        assert_eq!(consumed, 13);
    }

    #[test]
    fn parse_hostname() {
        let (host, consumed) = from_uri("example.com;transport=tcp").unwrap();
        assert_eq!(host, Host::Hostname("example.com".into()));
        assert_eq!(consumed, 11);
    }

    #[test]
    fn hostname_lowercased() {
        let (host, _) = from_uri("MY.DOMAIN").unwrap();
        assert_eq!(host, Host::Hostname("my.domain".into()));
    }

    #[test]
    fn parse_accepts_all_forms() {
        assert_eq!(
            Host::parse("example.test").unwrap(),
            Host::Hostname("example.test".into())
        );
        assert_eq!(
            Host::parse("192.0.2.1").unwrap(),
            Host::Ipv4(Ipv4Addr::new(192, 0, 2, 1))
        );
        let v6 = Host::Ipv6(
            "2001:db8::1"
                .parse::<Ipv6Addr>()
                .unwrap(),
        );
        assert_eq!(Host::parse("2001:db8::1").unwrap(), v6);
        assert_eq!(Host::parse("[2001:db8::1]").unwrap(), v6);
    }

    #[test]
    fn parse_round_trips_both_renderers() {
        for input in ["example.test", "192.0.2.1", "2001:db8::1", "[2001:db8::1]"] {
            let host = Host::parse(input).unwrap();
            assert_eq!(Host::parse(&host.to_string()).unwrap(), host);
            assert_eq!(
                Host::parse(
                    &host
                        .bare()
                        .to_string()
                )
                .unwrap(),
                host
            );
        }
        let host = Host::parse("[2001:db8::1]").unwrap();
        assert_eq!(host.to_string(), "[2001:db8::1]");
        assert_eq!(
            host.bare()
                .to_string(),
            "2001:db8::1"
        );
    }

    #[test]
    fn parse_rejects_only_unreadable_hosts() {
        assert_eq!(Host::parse(""), Err(ParseError::Empty));
        for input in ["[", "[192.0.2.1]", "[2001:db8::1", ":5060"] {
            assert_eq!(
                Host::parse(input),
                Err(ParseError::NoHost),
                "expected rejection of {input:?}"
            );
        }
        for input in [
            "not a host:",
            "example.test:5060",
            "example.test;transport=tcp",
        ] {
            assert!(
                Host::parse_with_warnings(input)
                    .unwrap()
                    .has_warnings(),
                "expected warnings for {input:?}"
            );
        }
    }

    #[test]
    fn a_zone_is_dropped_from_an_ipv6_address() {
        let addr = Host::Ipv6(
            "fe80::1"
                .parse()
                .unwrap(),
        );
        for input in ["fe80::1%eth0", "[fe80::1%25eth0]"] {
            let parsed = Host::parse_with_warnings(input).unwrap();
            assert_eq!(parsed.value, addr, "{input}");
            assert_eq!(
                parsed
                    .warnings
                    .len(),
                1,
                "{input}"
            );
            assert_eq!(parsed.warnings[0].code, WarningCode::Ipv6Zone);
            assert_eq!(parsed.warnings[0].position, input.find('%'));
        }
        let sip = crate::SipUri::parse_with_warnings("sip:alice@[fe80::1%25eth0]:5060").unwrap();
        assert_eq!(
            sip.value
                .host(),
            Some(&addr)
        );
        assert_eq!(
            sip.value
                .port(),
            Some(5060)
        );
        assert_eq!(sip.warnings[0].code, WarningCode::Ipv6Zone);
        assert_eq!(Host::parse("[fe80::zz%1]"), Err(ParseError::NoHost));
    }

    #[test]
    fn empty_host_is_none() {
        assert!(from_uri("").is_none());
        assert!(from_uri(":5060").is_none());
    }
}
