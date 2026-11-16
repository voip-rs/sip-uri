use std::fmt;

use crate::canon;
use crate::host::Host;
use crate::params::{self, Headers, Params, UserParams};

/// SIP or SIPS URI per RFC 3261 §19.
///
/// Parse text into one with sip-uri's `UriParse`.
///
/// Supports the full grammar including user-params (`;` within userinfo),
/// password, IPv6 hosts, URI parameters, and headers. A scheme or host that
/// is missing or unreadable is `None`.
///
/// [`fmt::Debug`] writes a password as `***`.
#[derive(Clone, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(from = "SipUriParts", into = "SipUriParts")
)]
#[non_exhaustive]
pub struct SipUri {
    scheme: Option<Scheme>,
    user: Option<String>,
    user_params: UserParams,
    password: Option<String>,
    host: Option<Host>,
    port: Option<u16>,
    params: Params,
    headers: Headers,
    fragment: Option<String>,
}

/// The components of a [`SipUri`], each canonized on conversion.
///
/// [`fmt::Debug`] writes a password as `***`.
#[derive(Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct SipUriParts {
    /// `sip` or `sips`.
    pub scheme: Option<Scheme>,
    /// The user part, without user-params or password.
    pub user: Option<String>,
    /// Parameters within the userinfo, before `@`.
    pub user_params: UserParams,
    /// The password.
    pub password: Option<String>,
    /// The host.
    pub host: Option<Host>,
    /// The port.
    pub port: Option<u16>,
    /// URI parameters after the host.
    pub params: Params,
    /// URI headers after `?`.
    pub headers: Headers,
    /// The fragment after `#`.
    pub fragment: Option<String>,
}

impl From<SipUriParts> for SipUri {
    fn from(p: SipUriParts) -> Self {
        SipUri {
            scheme: p.scheme,
            user: hold_user(
                p.user
                    .as_deref()
                    .map(canon::canonize_user),
                &p.user_params,
            ),
            user_params: p.user_params,
            password: p
                .password
                .as_deref()
                .map(canon::canonize_password),
            host: p
                .host
                .and_then(hold_host),
            port: p.port,
            params: p.params,
            headers: p.headers,
            fragment: p
                .fragment
                .as_deref()
                .and_then(hold_fragment),
        }
    }
}

impl From<SipUri> for SipUriParts {
    fn from(uri: SipUri) -> Self {
        uri.into_parts()
    }
}

struct Masked;

impl fmt::Debug for Masked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// The derived `Debug` field list of `$value`, its password masked.
macro_rules! debug_masking_password {
    ($f:expr, $name:literal, $value:expr) => {
        $f.debug_struct($name)
            .field("scheme", &$value.scheme)
            .field("user", &$value.user)
            .field("user_params", &$value.user_params)
            .field(
                "password",
                &$value
                    .password
                    .as_ref()
                    .map(|_| Masked),
            )
            .field("host", &$value.host)
            .field("port", &$value.port)
            .field("params", &$value.params)
            .field("headers", &$value.headers)
            .field("fragment", &$value.fragment)
            .finish()
    };
}

impl fmt::Debug for SipUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_masking_password!(f, "SipUri", self)
    }
}

impl fmt::Debug for SipUriParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        debug_masking_password!(f, "SipUriParts", self)
    }
}

/// An empty user prints as a bare `@`, which parses as no userinfo, unless
/// user-params follow it; user-params print only after a user.
fn hold_user(user: Option<String>, user_params: &UserParams) -> Option<String> {
    match user {
        Some(u) if u.is_empty() && user_params.is_empty() => None,
        None if !user_params.is_empty() => Some(String::new()),
        user => user,
    }
}

fn hold_host(host: Host) -> Option<Host> {
    Some(host.normalized())
        .filter(|h| !matches!(h, Host::Hostname(name) if name.as_str().is_empty()))
}

fn hold_fragment(fragment: &str) -> Option<String> {
    (!fragment.is_empty()).then(|| canon::canonize_sip_fragment(fragment))
}

/// SIP URI scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize),
    serde(rename_all = "lowercase")
)]
pub enum Scheme {
    /// `sip:` (default port 5060)
    Sip,
    /// `sips:` (default port 5061)
    Sips,
}

impl Scheme {
    /// The scheme name as it appears before `:`, lowercase.
    pub fn as_str(self) -> &'static str {
        match self {
            Scheme::Sip => "sip",
            Scheme::Sips => "sips",
        }
    }

    /// The port a URI without one implies, per RFC 3261 §19.1.1: 5060 for
    /// `sip`, 5061 for `sips`. A `sip` URI reached over TLS defaults to 5061.
    pub fn default_port(self) -> u16 {
        match self {
            Scheme::Sip => 5060,
            Scheme::Sips => 5061,
        }
    }
}

impl fmt::Display for Scheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl SipUri {
    /// Create a new SIP URI with the given host and `sip:` scheme.
    pub fn new(host: Host) -> Self {
        SipUriParts {
            scheme: Some(Scheme::Sip),
            host: Some(host),
            ..Default::default()
        }
        .into()
    }

    /// The components, in canonical form.
    pub fn into_parts(self) -> SipUriParts {
        SipUriParts {
            scheme: self.scheme,
            user: self.user,
            user_params: self.user_params,
            password: self.password,
            host: self.host,
            port: self.port,
            params: self.params,
            headers: self.headers,
            fragment: self.fragment,
        }
    }

    /// Set the URI scheme.
    pub fn with_scheme(mut self, scheme: Scheme) -> Self {
        self.scheme = Some(scheme);
        self
    }

    /// Set the user part, canonized like parsed text: a delimiter or byte
    /// outside the user grammar is escaped, so it cannot add user-params.
    pub fn with_user(mut self, user: impl AsRef<str>) -> Self {
        self.user = hold_user(Some(canon::canonize_user(user.as_ref())), &self.user_params);
        self
    }

    /// Replace all user-params (parameters within the userinfo, before `@`).
    pub fn with_user_params(mut self, params: impl Into<UserParams>) -> Self {
        self.user_params = params.into();
        self.user = hold_user(self.user, &self.user_params);
        self
    }

    /// Add a single user-param (parameter within the userinfo, before `@`),
    /// escaping any delimiter in the name or value.
    pub fn with_user_param(mut self, name: impl AsRef<str>, value: Option<&str>) -> Self {
        self.user_params
            .push(name, value);
        self.user = hold_user(self.user, &self.user_params);
        self
    }

    /// Set the password, escaping any delimiter in it.
    pub fn with_password(mut self, password: impl AsRef<str>) -> Self {
        self.password = Some(canon::canonize_password(password.as_ref()));
        self
    }

    /// Set the host, held as [`From`] a parts struct holds it: a dotted-quad
    /// hostname as [`Host::IPv4`], an empty one as no host.
    pub fn with_host(mut self, host: Host) -> Self {
        self.host = hold_host(host);
        self
    }

    /// Set the port.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Add a URI parameter, escaping any delimiter in the name or value.
    pub fn with_param(mut self, name: impl AsRef<str>, value: Option<&str>) -> Self {
        self.params
            .push(name, value);
        self
    }

    /// Add a header, escaping any delimiter in the name or value. `None`
    /// writes the name alone, without `=`.
    pub fn with_header(mut self, name: impl AsRef<str>, value: Option<&str>) -> Self {
        self.headers
            .push(name, value);
        self
    }

    /// Set the fragment component, escaping any delimiter in it.
    pub fn with_fragment(mut self, fragment: impl AsRef<str>) -> Self {
        self.fragment = hold_fragment(fragment.as_ref());
        self
    }

    /// The URI scheme (`sip` or `sips`), `None` when the input had none.
    pub fn scheme(&self) -> Option<Scheme> {
        self.scheme
    }

    /// The user part (without user-params or password).
    pub fn user(&self) -> Option<&str> {
        self.user
            .as_deref()
    }

    /// Parameters within the userinfo (before `@`), separated by `;` in the user part.
    ///
    /// Common in tel-style SIP URIs, e.g., `sip:+15551234567;cpc=ordinary@host`.
    pub fn user_params(&self) -> &UserParams {
        &self.user_params
    }

    /// The password component (deprecated by RFC 3261 but still parseable).
    pub fn password(&self) -> Option<&str> {
        self.password
            .as_deref()
    }

    /// The host, `None` when missing or unreadable.
    pub fn host(&self) -> Option<&Host> {
        self.host
            .as_ref()
    }

    /// The explicit port, if specified.
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// URI parameters (after host, separated by `;`).
    pub fn params(&self) -> &Params {
        &self.params
    }

    /// The URI parameters, to edit in place; every insertion canonizes.
    pub fn params_mut(&mut self) -> &mut Params {
        &mut self.params
    }

    /// The user-params, to edit in place; every insertion canonizes.
    ///
    /// When the guard drops, an absent user becomes empty if user-params
    /// remain, and an empty one absent if none do, as
    /// [`with_user_params`](Self::with_user_params) holds them.
    pub fn user_params_mut(&mut self) -> UserParamsMut<'_> {
        UserParamsMut(self)
    }

    /// The URI headers, to edit in place; every insertion canonizes.
    pub fn headers_mut(&mut self) -> &mut Headers {
        &mut self.headers
    }

    /// Look up a URI parameter by name (case-insensitive): `Some(None)` when
    /// it has no value.
    pub fn param(&self, name: &str) -> Option<Option<&str>> {
        self.params
            .get(name)
    }

    /// Look up a user-param by name (case-insensitive): `Some(None)` when it
    /// has no value.
    pub fn user_param(&self, name: &str) -> Option<Option<&str>> {
        self.user_params
            .get(name)
    }

    /// URI headers (after `?`).
    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    /// Look up a header by name (case-insensitive): `Some(None)` when it was
    /// written without `=`, `Some(Some(""))` when written `name=`.
    pub fn header(&self, name: &str) -> Option<Option<&str>> {
        self.headers
            .get(name)
    }

    /// The fragment component (after `#`), if present.
    ///
    /// RFC 3261 does not define fragments for SIP URIs, but sofia-sip
    /// and real-world implementations accept them permissively.
    pub fn fragment(&self) -> Option<&str> {
        self.fragment
            .as_deref()
    }

    /// Render `user@host:port` as [`fmt::Display`] writes those components,
    /// each absent one left out: no scheme, user-params, password, params,
    /// headers or fragment.
    ///
    /// ```
    /// use sip_uri_types::{Host, SipUri};
    ///
    /// let uri = SipUri::new(Host::IPv6("2001:db8::1".parse().unwrap()))
    ///     .with_user("alice")
    ///     .with_user_param("cpc", Some("ordinary"))
    ///     .with_password("pw")
    ///     .with_port(5060)
    ///     .with_param("transport", Some("tcp"));
    /// assert_eq!(uri.user_host().to_string(), "alice@[2001:db8::1]:5060");
    /// ```
    pub fn user_host(&self) -> UserHost<'_> {
        UserHost(self)
    }
}

/// Mutable access to a [`SipUri`]'s user-params, returned by
/// [`SipUri::user_params_mut`]; the user part is held again on drop.
#[derive(Debug)]
pub struct UserParamsMut<'a>(&'a mut SipUri);

impl std::ops::Deref for UserParamsMut<'_> {
    type Target = UserParams;

    fn deref(&self) -> &UserParams {
        &self
            .0
            .user_params
    }
}

impl std::ops::DerefMut for UserParamsMut<'_> {
    fn deref_mut(&mut self) -> &mut UserParams {
        &mut self
            .0
            .user_params
    }
}

impl Drop for UserParamsMut<'_> {
    fn drop(&mut self) {
        let uri = &mut *self.0;
        uri.user = hold_user(
            uri.user
                .take(),
            &uri.user_params,
        );
    }
}

/// [`fmt::Display`] adapter returned by [`SipUri::user_host`].
#[derive(Debug, Clone, Copy)]
pub struct UserHost<'a>(&'a SipUri);

impl fmt::Display for UserHost<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let uri = self.0;
        if let Some(ref user) = uri.user {
            write!(f, "{user}@")?;
        }
        if let Some(ref host) = uri.host {
            write!(f, "{host}")?;
        }
        if let Some(port) = uri.port {
            write!(f, ":{port}")?;
        }
        Ok(())
    }
}

impl fmt::Display for SipUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(scheme) = self.scheme {
            write!(f, "{scheme}:")?;
        }

        if let Some(ref user) = self.user {
            write!(f, "{user}")?;
            params::format_params(
                self.user_params
                    .iter(),
                f,
            )?;
            if let Some(ref pwd) = self.password {
                write!(f, ":{pwd}")?;
            }
            write!(f, "@")?;
        } else if let Some(ref pwd) = self.password {
            write!(f, ":{pwd}@")?;
        }

        if let Some(ref host) = self.host {
            write!(f, "{host}")?;
        }
        if let Some(port) = self.port {
            write!(f, ":{port}")?;
        }
        params::format_params(
            self.params
                .iter(),
            f,
        )?;
        params::format_headers(&self.headers, f)?;
        if let Some(ref frag) = self.fragment {
            write!(f, "#{frag}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder() {
        let uri = SipUri::new(Host::Hostname("example.com".into()))
            .with_user("alice")
            .with_param("transport", Some("tcp"));
        assert_eq!(uri.to_string(), "sip:alice@example.com;transport=tcp");
    }

    #[test]
    fn parts_canonize_like_builders() {
        let uri = SipUri::from(SipUriParts {
            scheme: Some(Scheme::Sip),
            user: Some("a b;c".into()),
            host: Some(Host::Hostname("example.com".into())),
            params: Params::new().with("x", Some("a@b")),
            ..Default::default()
        });
        let built = SipUri::new(Host::Hostname("example.com".into()))
            .with_user("a b;c")
            .with_param("x", Some("a@b"));
        assert_eq!(uri, built);
        assert_eq!(uri.user(), Some("a%20b%3Bc"));
        assert_eq!(uri.param("x"), Some(Some("a%40b")));
        assert_eq!(
            SipUri::from(
                uri.clone()
                    .into_parts()
            ),
            uri
        );
    }

    #[test]
    fn empty_components_are_absent() {
        let host = || Host::Hostname("example.com".into());
        let uri = SipUri::new(host())
            .with_user("")
            .with_param("", None)
            .with_fragment("");
        assert_eq!(uri, SipUri::new(host()));
        assert_eq!(uri.to_string(), "sip:example.com");
        assert_eq!(SipUri::new(Host::Hostname("".into())).host(), None);
    }

    #[test]
    fn dotted_quad_hostname_is_held_as_ipv4() {
        let built = SipUri::new(Host::Hostname("198.51.100.1".into()));
        assert_eq!(
            built.host(),
            Some(&Host::IPv4(std::net::Ipv4Addr::new(198, 51, 100, 1)))
        );
        let leading_zero = SipUri::new(Host::Hostname("198.051.100.1".into()));
        assert!(matches!(leading_zero.host(), Some(Host::Hostname(_))));
    }

    #[test]
    fn user_params_hold_an_empty_user() {
        let host = || Host::Hostname("example.com".into());
        let uri = SipUri::new(host())
            .with_user("")
            .with_user_param("cpc", Some("x"));
        assert_eq!(uri.user(), Some(""));
        assert_eq!(uri.to_string(), "sip:;cpc=x@example.com");
        let from_parts = SipUri::from(SipUriParts {
            scheme: Some(Scheme::Sip),
            user_params: UserParams::new().with("cpc", Some("x")),
            host: Some(host()),
            ..Default::default()
        });
        assert_eq!(from_parts, uri);
        assert_eq!(
            uri.with_user_params(UserParams::new())
                .user(),
            None
        );
    }

    #[test]
    fn debug_masks_the_password() {
        let uri = SipUri::new(Host::Hostname("example.com".into()))
            .with_user("alice")
            .with_password("secret");
        for debug in [
            format!("{uri:?}"),
            format!("{:?}", crate::Uri::Sip(uri.clone())),
            format!(
                "{:?}",
                uri.clone()
                    .into_parts()
            ),
        ] {
            assert!(!debug.contains("secret"), "{debug}");
            assert!(debug.contains("password: Some(***)"), "{debug}");
            assert!(debug.contains("user: Some(\"alice\")"), "{debug}");
        }
        let without = SipUri::new(Host::Hostname("example.com".into()));
        assert!(format!("{without:?}").contains("password: None"));
    }

    #[test]
    fn header_without_value_prints_its_name_alone() {
        let uri = SipUri::new(Host::Hostname("example.com".into()))
            .with_header("h", None)
            .with_header("e", Some(""));
        assert_eq!(uri.header("h"), Some(None));
        assert_eq!(uri.header("e"), Some(Some("")));
        assert_eq!(uri.to_string(), "sip:example.com?h&e=");
    }
}
