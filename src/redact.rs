//! Rendering a URI for logs.
//!
//! The defaults follow the rationale's "Redaction masks only what
//! identifies a person": a position the grammar gives a person's identity
//! is masked, public data stays, a position whose content this crate cannot
//! classify starts masked, and an identity a sender put off-grammar is
//! masked only on request. Every default is Display output of the adapter,
//! so changing one is a breaking release.

use std::fmt;
use std::sync::Arc;

use sip_uri_types::OtherUri;
use sip_uri_types::Pairs;
use sip_uri_types::Params;
use sip_uri_types::SipUri;
use sip_uri_types::TelUri;
use sip_uri_types::Uri;
use sip_uri_types::UrnUri;

use crate::grammar::ESCAPE_LEN;

const MASK: &str = "***";

/// How the user part, or a tel: number, is rendered by [`Redaction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UserMask {
    /// The whole userinfo, user-params and password included, becomes `***`.
    Full,
    /// Every digit outside a `%XX` escape but the last `n` becomes `x`;
    /// escapes and other characters stay. Suits telephone numbers.
    KeepLast(usize),
    /// The user part is shown as Display writes it.
    Visible,
}

/// How the headers of a SIP URI are rendered by [`Redaction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HeaderMask {
    /// Each value becomes `***`; names stay, and a header without a value
    /// stays its name alone.
    Values,
    /// Headers are shown as Display writes them.
    Visible,
    /// Headers are left out.
    Dropped,
}

/// How a URN, or a URI of an unrecognized scheme, is rendered by
/// [`Redaction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OpaqueMask {
    /// What may identify a person is masked, the rest shown. A URN in a
    /// [public kind](Redaction::public_urns) is shown whole; another
    /// becomes `urn:<nid>:***`, or `urn:<nid>` without an NSS. A
    /// hierarchical URI (`scheme://`) masks its userinfo and query,
    /// `https://***@example.com/a?***#f`; any other becomes `<scheme>:***`,
    /// or `***` without a scheme. A NID or scheme is shown as sent, even
    /// when a sender put an identity there.
    Masked,
    /// A URN becomes `urn:***` and another URI `***`, for senders that put
    /// an identity in the NID or scheme position.
    Full,
    /// Shown as Display writes it.
    Visible,
}

/// How the host of a SIP URI is rendered by [`Redaction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostMask {
    /// Shown as Display writes it, even when a sender put a number there.
    Visible,
    /// The host becomes `***`; the port stays.
    Masked,
}

/// What a redacted rendering masks.
///
/// [`Redaction::default`] masks the whole userinfo, or a tel: number, and
/// every URI header value, since headers such as `P-Asserted-Identity`
/// carry identities: `sip:***@example.com?Subject=***`. A URN outside the
/// [public kinds](Redaction::public_urns) and the userinfo and query of
/// an unrecognized URI are masked too; see [`OpaqueMask::Masked`]. Params
/// are shown unless named in [`Redaction::params`], except that an `ext` or
/// `isub` on a tel: URI or among SIP user-params, part of the subscriber's
/// number, is masked by the [`UserMask`] as the number is. Hosts and
/// routing params such as `rn` or `cic` are public and stay shown; a number
/// a sender put in host or NID position shows unless [`HostMask::Masked`]
/// or [`OpaqueMask::Full`] masks it. A password is `***` under every
/// [`UserMask`].
///
/// A policy owns its data and clones cheaply, so one read from configuration
/// is built once and lent to every rendering:
///
/// ```
/// use sip_uri::{Redaction, SipUri, UriParse, UriRedact, UserMask};
///
/// struct Logger {
///     redaction: Redaction,
/// }
///
/// impl Logger {
///     fn new(masked_params: &[String]) -> Self {
///         let redaction = Redaction::default()
///             .user(UserMask::KeepLast(4))
///             .params(masked_params);
///         Logger { redaction }
///     }
///
///     fn line(&self, uri: &SipUri) -> String {
///         format!("call from {}", uri.redacted(&self.redaction))
///     }
/// }
///
/// let logger = Logger::new(&["session".to_string()]);
/// let uri = SipUri::parse("sip:+15551234567@example.com;session=7").unwrap();
/// assert_eq!(
///     logger.line(&uri),
///     "call from sip:+xxxxxxx4567@example.com;session=***"
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Redaction {
    user: UserMask,
    headers: HeaderMask,
    params: Arc<[String]>,
    urn: OpaqueMask,
    other: OpaqueMask,
    host: HostMask,
    public_urns: Arc<[String]>,
}

impl Default for Redaction {
    fn default() -> Self {
        Redaction {
            user: UserMask::Full,
            headers: HeaderMask::Values,
            params: Arc::from([]),
            urn: OpaqueMask::Masked,
            other: OpaqueMask::Masked,
            host: HostMask::Visible,
            public_urns: ["service", "nena:service", "emergency:uid"]
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }
}

/// Whether `urn` falls under `entry`, a NID alone or `nid:nss-prefix`. The
/// NID compares ignoring ASCII case, the NSS prefix exactly and only up to
/// a `:` boundary.
fn urn_under(entry: &str, urn: &UrnUri) -> bool {
    let (nid, prefix) = match entry.split_once(':') {
        Some((nid, prefix)) => (nid, Some(prefix)),
        None => (entry, None),
    };
    let nid_matches = urn
        .nid()
        .is_some_and(|n| n.eq_ignore_ascii_case(nid));
    let nss = urn
        .nss()
        .unwrap_or_default();
    nid_matches
        && prefix.map_or(true, |p| {
            nss == p
                || nss
                    .strip_prefix(p)
                    .is_some_and(|rest| rest.starts_with(':'))
        })
}

impl Redaction {
    /// Show URNs of these kinds in full under [`OpaqueMask::Masked`], in
    /// place of the default list. An entry is a NID (`service`) or a NID with
    /// the leading NSS components of one kind (`nena:service`). The default
    /// lists `service` and `nena:service` service URNs, and `emergency:uid`
    /// call, incident and log identifiers. A kind is listed only when no URN
    /// of it identifies a person.
    pub fn public_urns<I, S>(mut self, kinds: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.public_urns = kinds
            .into_iter()
            .map(Into::into)
            .collect();
        self
    }

    /// Whether `urn` is of a kind [`public_urns`](Redaction::public_urns)
    /// lists.
    pub fn is_public_urn(&self, urn: &UrnUri) -> bool {
        self.public_urns
            .iter()
            .any(|entry| urn_under(entry, urn))
    }

    /// Set how the host of a SIP URI is rendered.
    pub fn host(mut self, mask: HostMask) -> Self {
        self.host = mask;
        self
    }

    /// How the host of a SIP URI is rendered.
    pub fn host_mask(&self) -> HostMask {
        self.host
    }

    /// Set how a URN is rendered.
    pub fn urn(mut self, mask: OpaqueMask) -> Self {
        self.urn = mask;
        self
    }

    /// How a URN is rendered.
    pub fn urn_mask(&self) -> OpaqueMask {
        self.urn
    }

    /// Set how a URI of an unrecognized scheme, or of none, is rendered.
    pub fn other(mut self, mask: OpaqueMask) -> Self {
        self.other = mask;
        self
    }

    /// How a URI of an unrecognized scheme, or of none, is rendered.
    pub fn other_mask(&self) -> OpaqueMask {
        self.other
    }

    /// Set how the user part or tel: number is rendered.
    pub fn user(mut self, mask: UserMask) -> Self {
        self.user = mask;
        self
    }

    /// How the user part or tel: number is rendered.
    pub fn user_mask(&self) -> UserMask {
        self.user
    }

    /// Set how URI headers are rendered.
    pub fn headers(mut self, mask: HeaderMask) -> Self {
        self.headers = mask;
        self
    }

    /// How URI headers are rendered.
    pub fn header_mask(&self) -> HeaderMask {
        self.headers
    }

    /// Leave URI headers out of the rendering: [`HeaderMask::Dropped`].
    pub fn drop_headers(self) -> Self {
        self.headers(HeaderMask::Dropped)
    }

    /// Render the values of params with these names, canonized and compared
    /// case-insensitively, as `***`, in place of any set before.
    pub fn params<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let canonical: Params = names
            .into_iter()
            .map(|name| (name, None::<&str>))
            .collect();
        self.params = canonical
            .iter()
            .map(|(name, _)| name.to_string())
            .collect();
        self
    }

    /// Whether [`params`](Redaction::params) names `name`, ignoring ASCII case.
    pub fn masks_param(&self, name: &str) -> bool {
        self.params
            .iter()
            .any(|p| p.eq_ignore_ascii_case(name))
    }
}

/// [`fmt::Display`] adapter returned by [`UriRedact::redacted`]. Its Debug
/// writes the same redacted text, so `{:?}` in a log line leaks nothing.
#[derive(Clone, Copy)]
pub struct Redacted<'a, T> {
    uri: &'a T,
    how: &'a Redaction,
}

impl<T> fmt::Debug for Redacted<'_, T>
where
    Self: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Redacted")
            .field(&format_args!("{self}"))
            .finish()
    }
}

mod sealed {
    pub trait Sealed {}
}

/// Rendering for logs, for [`Uri`] and each of its variants' types.
pub trait UriRedact: Sized + sealed::Sealed {
    /// Render for logs, masking what `how` names.
    ///
    /// ```
    /// use sip_uri::{Redaction, SipUri, UriParse, UriRedact, UserMask};
    ///
    /// let uri = SipUri::parse("sip:+15551234567;cpc=ordinary:pw@example.com").unwrap();
    /// assert_eq!(uri.redacted(&Redaction::default()).to_string(), "sip:***@example.com");
    /// assert_eq!(
    ///     uri.redacted(&Redaction::default().user(UserMask::KeepLast(4))).to_string(),
    ///     "sip:+xxxxxxx4567;cpc=ordinary:***@example.com"
    /// );
    /// ```
    fn redacted<'a>(&'a self, how: &'a Redaction) -> Redacted<'a, Self>;
}

macro_rules! impl_uri_redact {
    ($($ty:ty),*) => {$(
        impl sealed::Sealed for $ty {}

        impl UriRedact for $ty {
            fn redacted<'a>(&'a self, how: &'a Redaction) -> Redacted<'a, Self> {
                Redacted { uri: self, how }
            }
        }
    )*};
}

impl_uri_redact!(SipUri, TelUri, UrnUri, OtherUri, Uri);

fn write_masked_user(f: &mut fmt::Formatter<'_>, user: &str, mask: UserMask) -> fmt::Result {
    match mask {
        UserMask::Full => f.write_str(MASK),
        UserMask::Visible => f.write_str(user),
        UserMask::KeepLast(n) => {
            let mut masked = units(user)
                .filter(is_digit)
                .count()
                .saturating_sub(n);
            for unit in units(user) {
                if masked > 0 && is_digit(&unit) {
                    masked -= 1;
                    f.write_str("x")?;
                } else {
                    f.write_str(unit)?;
                }
            }
            Ok(())
        }
    }
}

/// `text` split into well-formed `%XX` escapes and single characters.
fn units(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let len = match rest.as_bytes() {
            [] => return None,
            [b'%', hi, lo, ..] if hi.is_ascii_hexdigit() && lo.is_ascii_hexdigit() => ESCAPE_LEN,
            _ => rest
                .chars()
                .next()
                .map_or(1, char::len_utf8),
        };
        let (unit, tail) = rest.split_at(len);
        rest = tail;
        Some(unit)
    })
}

fn is_digit(unit: &&str) -> bool {
    matches!(unit.as_bytes(), [d] if d.is_ascii_digit())
}

/// Whether a tel: param is part of the subscriber's own number: an
/// extension or a subaddress. Routing and carrier params name a network,
/// not the caller, and stay visible for debugging.
fn carries_number(name: &str) -> bool {
    name.eq_ignore_ascii_case("ext") || name.eq_ignore_ascii_case("isub")
}

/// Write params, masking named ones; with `tel` set, those that
/// [carry a number](carries_number) are masked as the number is.
fn write_params(
    f: &mut fmt::Formatter<'_>,
    params: Pairs<'_>,
    how: &Redaction,
    tel: bool,
) -> fmt::Result {
    for (name, value) in params {
        write!(f, ";{name}")?;
        match value {
            Some(_) if how.masks_param(name) => write!(f, "={MASK}")?,
            Some(v) if tel && carries_number(name) => {
                f.write_str("=")?;
                write_masked_user(f, v, how.user)?;
            }
            Some(v) => write!(f, "={v}")?,
            None => {}
        }
    }
    Ok(())
}

impl fmt::Display for Redacted<'_, SipUri> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (uri, how) = (self.uri, self.how);
        if let Some(scheme) = uri.scheme() {
            write!(f, "{scheme}:")?;
        }
        if uri
            .user()
            .is_some()
            || uri
                .password()
                .is_some()
        {
            if how.user == UserMask::Full {
                write!(f, "{MASK}@")?;
            } else {
                if let Some(user) = uri.user() {
                    write_masked_user(f, user, how.user)?;
                    write_params(
                        f,
                        uri.user_params()
                            .iter(),
                        how,
                        true,
                    )?;
                }
                if uri
                    .password()
                    .is_some()
                {
                    write!(f, ":{MASK}")?;
                }
                f.write_str("@")?;
            }
        }
        match (uri.host(), how.host) {
            (Some(_), HostMask::Masked) => f.write_str(MASK)?,
            (Some(host), HostMask::Visible) => write!(f, "{host}")?,
            (None, _) => {}
        }
        if let Some(port) = uri.port() {
            write!(f, ":{port}")?;
        }
        write_params(
            f,
            uri.params()
                .iter(),
            how,
            false,
        )?;
        if how.headers != HeaderMask::Dropped {
            for (i, (name, value)) in uri
                .headers()
                .iter()
                .enumerate()
            {
                let sep = if i == 0 { '?' } else { '&' };
                write!(f, "{sep}{name}")?;
                match value {
                    Some(_) if how.headers == HeaderMask::Values => write!(f, "={MASK}")?,
                    Some(v) => write!(f, "={v}")?,
                    None => {}
                }
            }
        }
        if let Some(frag) = uri.fragment() {
            write!(f, "#{frag}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Redacted<'_, TelUri> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (uri, how) = (self.uri, self.how);
        f.write_str("tel:")?;
        if let Some(number) = uri.number() {
            write_masked_user(f, number, how.user)?;
        }
        write_params(
            f,
            uri.params()
                .iter(),
            how,
            true,
        )?;
        if let Some(frag) = uri.fragment() {
            write!(f, "#{frag}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Redacted<'_, Uri> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.uri {
            Uri::Sip(u) => write!(f, "{}", u.redacted(self.how)),
            Uri::Tel(u) => write!(f, "{}", u.redacted(self.how)),
            Uri::Urn(u) => write!(f, "{}", u.redacted(self.how)),
            Uri::Other(o) => write!(f, "{}", o.redacted(self.how)),
        }
    }
}

impl fmt::Display for Redacted<'_, UrnUri> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (
            self.how
                .urn,
            self.uri
                .nid(),
        ) {
            (OpaqueMask::Visible, _) => write!(f, "{}", self.uri),
            (OpaqueMask::Masked, Some(_))
                if self
                    .how
                    .is_public_urn(self.uri) =>
            {
                write!(f, "{}", self.uri)
            }
            (OpaqueMask::Masked, Some(nid))
                if self
                    .uri
                    .nss()
                    .is_none() =>
            {
                write!(f, "urn:{nid}")
            }
            (OpaqueMask::Masked, Some(nid)) => write!(f, "urn:{nid}:{MASK}"),
            (OpaqueMask::Masked | OpaqueMask::Full, _) => write!(f, "urn:{MASK}"),
        }
    }
}

impl fmt::Display for Redacted<'_, OtherUri> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (
            self.how
                .other,
            self.uri
                .scheme(),
        ) {
            (OpaqueMask::Visible, _) => write!(f, "{}", self.uri),
            (OpaqueMask::Masked, Some(scheme)) => {
                let rest = self
                    .uri
                    .as_str()
                    .get(scheme.len() + 1..)
                    .unwrap_or_default();
                write_masked_other(f, scheme, rest)
            }
            (OpaqueMask::Masked | OpaqueMask::Full, _) => f.write_str(MASK),
        }
    }
}

/// `scheme://authority/path?query#fragment` with the userinfo and query
/// masked; a URI without `//` has no part to locate and is masked whole.
fn write_masked_other(f: &mut fmt::Formatter<'_>, scheme: &str, rest: &str) -> fmt::Result {
    let Some(hier) = rest.strip_prefix("//") else {
        return write!(f, "{scheme}:{MASK}");
    };
    let authority_end = hier
        .find(['/', '?', '#'])
        .unwrap_or(hier.len());
    let (authority, tail) = hier.split_at(authority_end);
    write!(f, "{scheme}://")?;
    match authority.rsplit_once('@') {
        Some((_, host)) => write!(f, "{MASK}@{host}")?,
        None => f.write_str(authority)?,
    }
    let fragment_start = tail
        .find('#')
        .unwrap_or(tail.len());
    let (before_fragment, fragment) = tail.split_at(fragment_start);
    match before_fragment.split_once('?') {
        Some((path, _)) => write!(f, "{path}?{MASK}")?,
        None => f.write_str(before_fragment)?,
    }
    f.write_str(fragment)
}
