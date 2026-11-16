use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::canon;

/// Host component of a SIP URI.
///
/// Parse text into one with sip-uri's `UriParse`.
///
/// IPv6 addresses are stored without brackets; [`fmt::Display`] adds them,
/// and [`Host::bare`] renders without. A URI holds a hostname that reads as
/// an IPv4 address as [`Host::Ipv4`], as [`Host::from_hostname`] does.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "lowercase")
)]
#[non_exhaustive]
pub enum Host {
    /// IPv4 address.
    Ipv4(Ipv4Addr),
    /// IPv6 address (stored without brackets).
    Ipv6(Ipv6Addr),
    /// DNS hostname.
    ///
    /// Built directly from an empty or dotted-quad name, this host prints as
    /// text the parser reads as no host or as [`Host::Ipv4`]. URI
    /// constructors and serde hold a dotted quad as [`Host::Ipv4`]; see
    /// [`Host::from_hostname`].
    Hostname(Hostname),
}

/// DNS hostname in canonical form, so equal names compare and hash equal
/// however they were written.
///
/// Construction lowercases, decodes escaped unreserved characters and escapes
/// every other byte outside them; it does not otherwise validate. An empty or
/// dotted-quad name is a value too, but not one the parser returns: it reads
/// that text as no host or as [`Host::Ipv4`], as URI constructors and serde
/// hold a dotted quad.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hostname(String);

impl Hostname {
    /// The hostname, lowercase.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for Hostname {
    fn from(s: String) -> Self {
        Hostname(canon::canonize_hostname(s))
    }
}

impl From<&str> for Hostname {
    fn from(s: &str) -> Self {
        Hostname(canon::canonize_hostname(s))
    }
}

impl fmt::Display for Hostname {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Host {
    /// [`Host::Ipv4`] when the canonical name is a dotted-quad address as
    /// [`Ipv4Addr`]'s `FromStr` reads it, leading zeros refused; otherwise
    /// [`Host::Hostname`].
    ///
    /// URI constructors hold every host this way, so a URI built from
    /// `Host::Hostname("198.51.100.1".into())` equals the parsed one.
    ///
    /// ```
    /// use std::net::Ipv4Addr;
    /// use sip_uri_types::Host;
    ///
    /// assert_eq!(
    ///     Host::from_hostname("198.51.100.1"),
    ///     Host::Ipv4(Ipv4Addr::new(198, 51, 100, 1))
    /// );
    /// assert!(matches!(Host::from_hostname("198.051.100.1"), Host::Hostname(_)));
    /// ```
    pub fn from_hostname(name: impl Into<Hostname>) -> Host {
        let name = name.into();
        // Every address Ipv4Addr parses starts with a digit; other names skip it.
        let addr = name
            .as_str()
            .starts_with(|c: char| c.is_ascii_digit())
            .then(|| {
                name.as_str()
                    .parse::<Ipv4Addr>()
            });
        match addr {
            Some(Ok(addr)) => Host::Ipv4(addr),
            _ => Host::Hostname(name),
        }
    }

    /// The host as URI constructors hold it: see [`Host::from_hostname`].
    pub(crate) fn normalized(self) -> Host {
        match self {
            Host::Hostname(name) => Host::from_hostname(name),
            host => host,
        }
    }

    /// Render without IPv6 brackets, for contexts that supply their own or take
    /// no brackets at all (SDP connection lines, URI parameter values, log text).
    ///
    /// ```
    /// use sip_uri_types::Host;
    ///
    /// let host = Host::Ipv6("2001:db8::1".parse().unwrap());
    /// assert_eq!(host.to_string(), "[2001:db8::1]");
    /// assert_eq!(host.bare().to_string(), "2001:db8::1");
    /// ```
    pub fn bare(&self) -> Bare<'_> {
        Bare(self)
    }

    /// The address of an IPv4 or IPv6 host, `None` for a hostname.
    pub fn ip(&self) -> Option<IpAddr> {
        match self {
            Host::Ipv4(addr) => Some(IpAddr::V4(*addr)),
            Host::Ipv6(addr) => Some(IpAddr::V6(*addr)),
            Host::Hostname(_) => None,
        }
    }
}

impl From<Ipv4Addr> for Host {
    fn from(addr: Ipv4Addr) -> Self {
        Host::Ipv4(addr)
    }
}

impl From<Ipv6Addr> for Host {
    fn from(addr: Ipv6Addr) -> Self {
        Host::Ipv6(addr)
    }
}

impl From<IpAddr> for Host {
    fn from(addr: IpAddr) -> Self {
        match addr {
            IpAddr::V4(v4) => Host::Ipv4(v4),
            IpAddr::V6(v6) => Host::Ipv6(v6),
        }
    }
}

impl AsRef<str> for Hostname {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// [`fmt::Display`] wrapper that renders IPv6 without brackets.
///
/// Returned by [`Host::bare`].
#[derive(Debug, Clone, Copy)]
pub struct Bare<'a>(&'a Host);

impl fmt::Display for Bare<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Host::Ipv4(addr) => write!(f, "{addr}"),
            Host::Ipv6(addr) => write!(f, "{addr}"),
            Host::Hostname(name) => f.write_str(name.as_str()),
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Host::Ipv4(addr) => write!(f, "{addr}"),
            Host::Ipv6(addr) => write!(f, "[{addr}]"),
            Host::Hostname(name) => f.write_str(name.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_ipv6_has_brackets() {
        let host = Host::Ipv6(Ipv6Addr::LOCALHOST);
        assert_eq!(host.to_string(), "[::1]");
    }

    #[test]
    fn ip_addresses_convert_both_ways() {
        let v4 = Ipv4Addr::new(198, 51, 100, 1);
        let v6 = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
        for addr in [IpAddr::V4(v4), IpAddr::V6(v6)] {
            assert_eq!(Host::from(addr).ip(), Some(addr));
        }
        assert_eq!(Host::from(v4), Host::Ipv4(v4));
        assert_eq!(Host::from(v6), Host::Ipv6(v6));
        assert_eq!(Host::Hostname("example.com".into()).ip(), None);
        let name = Hostname::from("EXAMPLE.com");
        assert_eq!(AsRef::<str>::as_ref(&name), name.as_str());
    }
}
