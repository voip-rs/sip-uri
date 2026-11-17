//! Rendering a URI for logs.
//!
//! Display carries the user part and password, so logs use this rendering,
//! never Display. What a log may carry is the deployment's policy, so
//! redaction lives here, not with the value, and every mask is the caller's
//! to relax or tighten. The redacted text is a Display adapter's output, so
//! changing a default mask ships only in a breaking release.
//!
//! The default masks only what identifies a person and leaves what is
//! public visible, since a log that hides the network cannot debug a call:
//! hosts, routing and carrier params, namespaces and schemes name networks
//! and services, not callers. A position is masked when the grammar puts a
//! person's identity there: the userinfo or tel: number with the params that
//! are part of a subscriber's number, and every URI header value, since
//! headers carry identities such as an asserted caller.
//!
//! Where whether content is personal depends on a registry or a grammar
//! this crate does not parse, the mask starts closed. A URN's NSS shows only
//! when the URN is of a kind on the public list, a kind being a namespace or
//! a subtree of one, since a namespace such as NENA's mixes public service
//! names with kinds nobody here has vetted. Another URI masks the userinfo
//! and query of a hierarchical form, or everything after the scheme when no
//! part can be located.
//!
//! The default never chases an identity a sender put where the grammar
//! places none, such as a number in host or namespace position: that sender
//! owns the exposure, and masking those positions is an option the caller
//! turns on. The default also shows what lies outside the positions above:
//! a SIP or tel: fragment, which runs from the first `#` to the end, and a
//! URL's fragment, which `FragmentMask::Full` masks; the name of a header
//! written without a value; and a URL's path, which only
//! `OpaqueMask::Full` hides. Text with no readable scheme, such as `*` or a
//! bare host and port, is `Other` and renders as `***`.
//!
//! A password is masked under every mask, as a log must never carry one:
//! a SIP password under every `UserMask`, and in another URI shown under
//! `OpaqueMask::Visible`, the password sip-uri-types' `OtherUri` Debug masks.
//! The span finder is kept here rather than exported from the data crate,
//! so it mirrors that Debug's rule, and tests/redact.rs compares the two
//! renderings over one table: a change to either rule changes both.

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
    /// escapes and other characters stay. Suits telephone numbers. An `ext`
    /// or `isub` is masked on its own, so one of `n` digits or fewer shows.
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
    /// [public kind](Redaction::with_public_urns) is shown whole; another
    /// becomes `urn:<nid>:***`, or `urn:<nid>` without an NSS. A
    /// hierarchical URI (`scheme://`) masks its userinfo and query,
    /// `https://***@example.com/a?***#f`; any other becomes `<scheme>:***`,
    /// or `***` without a scheme. A NID or scheme is shown as sent, even
    /// when a sender put an identity there.
    Masked,
    /// A URN becomes `urn:***` and another URI `***`, for senders that put
    /// an identity in the NID or scheme position.
    Full,
    /// Shown as Display writes it, except that a password in another URI
    /// is `***`, as its Debug writes it.
    Visible,
}

/// How the host of a SIP URI is rendered by [`Redaction`]. A host carried
/// in a param, such as `maddr`, is masked through [`Redaction::with_masked_params`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostMask {
    /// Shown as Display writes it, even when a sender put a number there.
    Visible,
    /// The host becomes `***`; the port stays.
    Full,
}

/// How a fragment is rendered by [`Redaction`]: a SIP or tel: fragment, and
/// a hierarchical URI's fragment under [`OpaqueMask::Masked`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FragmentMask {
    /// Shown as Display writes it.
    Visible,
    /// The fragment becomes `#***`.
    Full,
}

/// What a redacted rendering masks.
///
/// [`Redaction::default`] masks the whole userinfo, or a tel: number, and
/// every URI header value, since headers such as `P-Asserted-Identity`
/// carry identities: `sip:***@example.com?Subject=***`. A URN outside the
/// [public kinds](Redaction::with_public_urns) and the userinfo and query of
/// an unrecognized URI are masked too; see [`OpaqueMask::Masked`]. Params
/// are shown unless named in [`Redaction::with_masked_params`], except that an `ext` or
/// `isub` on a tel: URI or among SIP user-params, part of the subscriber's
/// number, is masked by the [`UserMask`] as the number is. Hosts and
/// routing params such as `rn` or `cic` are public and stay shown; a number
/// a sender put in host or NID position shows unless [`HostMask::Full`]
/// or [`OpaqueMask::Full`] masks it. A password is `***` under every
/// [`UserMask`]; when the scheme cannot be read, a `:` starts no password,
/// so that text stays in the user part and follows the user mask.
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
///             .with_user(UserMask::KeepLast(4))
///             .with_masked_params(masked_params);
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
pub struct Redaction {
    user: UserMask,
    headers: HeaderMask,
    params: Arc<Params>,
    urn: OpaqueMask,
    other: OpaqueMask,
    host: HostMask,
    fragment: FragmentMask,
    public_urns: Arc<[String]>,
}

impl Default for Redaction {
    fn default() -> Self {
        Redaction {
            user: UserMask::Full,
            headers: HeaderMask::Values,
            params: Arc::default(),
            urn: OpaqueMask::Masked,
            other: OpaqueMask::Masked,
            host: HostMask::Visible,
            fragment: FragmentMask::Visible,
            public_urns: [
                "service",
                "nena:service",
                "emergency:service",
                "emergency:uid",
            ]
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
    /// lists `service`, `nena:service` and `emergency:service` service URNs,
    /// and `emergency:uid` call, incident and log identifiers. A kind is
    /// listed only when no URN of it identifies a person. A URN of a public
    /// kind is shown whole, its r-, q- and f-components included.
    pub fn with_public_urns<I, S>(mut self, kinds: I) -> Self
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

    /// Whether `urn` is of a kind [`with_public_urns`](Redaction::with_public_urns)
    /// lists.
    pub fn is_public_urn(&self, urn: &UrnUri) -> bool {
        self.public_urns
            .iter()
            .any(|entry| urn_under(entry, urn))
    }

    /// Set how the host of a SIP URI is rendered.
    pub fn with_host(mut self, mask: HostMask) -> Self {
        self.host = mask;
        self
    }

    /// How the host of a SIP URI is rendered.
    pub fn host_mask(&self) -> HostMask {
        self.host
    }

    /// Set how a fragment is rendered.
    pub fn with_fragment(mut self, mask: FragmentMask) -> Self {
        self.fragment = mask;
        self
    }

    /// How a fragment is rendered.
    pub fn fragment_mask(&self) -> FragmentMask {
        self.fragment
    }

    /// Set how a URN is rendered.
    pub fn with_urn(mut self, mask: OpaqueMask) -> Self {
        self.urn = mask;
        self
    }

    /// How a URN is rendered.
    pub fn urn_mask(&self) -> OpaqueMask {
        self.urn
    }

    /// Set how a URI of an unrecognized scheme, or of none, is rendered.
    pub fn with_other(mut self, mask: OpaqueMask) -> Self {
        self.other = mask;
        self
    }

    /// How a URI of an unrecognized scheme, or of none, is rendered.
    pub fn other_mask(&self) -> OpaqueMask {
        self.other
    }

    /// Set how the user part or tel: number is rendered.
    pub fn with_user(mut self, mask: UserMask) -> Self {
        self.user = mask;
        self
    }

    /// How the user part or tel: number is rendered.
    pub fn user_mask(&self) -> UserMask {
        self.user
    }

    /// Set how URI headers are rendered.
    pub fn with_headers(mut self, mask: HeaderMask) -> Self {
        self.headers = mask;
        self
    }

    /// How URI headers are rendered.
    pub fn header_mask(&self) -> HeaderMask {
        self.headers
    }

    /// Render the values of params with these names, canonized and compared
    /// case-insensitively, as `***`, in place of any set before.
    pub fn with_masked_params<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let names: Params = names
            .into_iter()
            .map(|name| (name, None::<&str>))
            .collect();
        self.params = Arc::new(names);
        self
    }

    /// Whether [`with_masked_params`](Redaction::with_masked_params) names `name`, both canonized
    /// and compared ignoring ASCII case, as [`Params::get`] finds a name.
    pub fn masks_param(&self, name: &str) -> bool {
        self.params
            .get(name)
            .is_some()
    }
}

/// [`fmt::Display`] adapter returned by [`UriRedact::redacted`]. Its Debug
/// writes the same redacted text, so `{:?}` in a log line leaks nothing.
pub struct Redacted<'a, T> {
    uri: &'a T,
    how: &'a Redaction,
}

/// By hand: a derive would require `T: Copy`, which no URI type is.
impl<T> Clone for Redacted<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Redacted<'_, T> {}

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
    ///     uri.redacted(&Redaction::default().with_user(UserMask::KeepLast(4))).to_string(),
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
        f.write_str(";")?;
        f.write_str(name)?;
        match value {
            Some(_) if how.masks_param(name) => {
                f.write_str("=")?;
                f.write_str(MASK)?;
            }
            Some(v) if tel && carries_number(name) => {
                f.write_str("=")?;
                write_masked_user(f, v, how.user)?;
            }
            Some(v) => {
                f.write_str("=")?;
                f.write_str(v)?;
            }
            None => {}
        }
    }
    Ok(())
}

impl fmt::Display for Redacted<'_, SipUri> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (uri, how) = (self.uri, self.how);
        if let Some(scheme) = uri.scheme() {
            f.write_str(scheme.as_str())?;
            f.write_str(":")?;
        }
        if uri
            .user()
            .is_some()
            || uri
                .password()
                .is_some()
        {
            if how.user == UserMask::Full {
                f.write_str(MASK)?;
                f.write_str("@")?;
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
                    f.write_str(":")?;
                    f.write_str(MASK)?;
                }
                f.write_str("@")?;
            }
        }
        match (uri.host(), how.host) {
            (Some(_), HostMask::Full) => f.write_str(MASK)?,
            (Some(host), HostMask::Visible) => fmt::Display::fmt(host, f)?,
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
                let sep = if i == 0 { "?" } else { "&" };
                f.write_str(sep)?;
                f.write_str(name)?;
                match value {
                    Some(_) if how.headers == HeaderMask::Values => {
                        f.write_str("=")?;
                        f.write_str(MASK)?;
                    }
                    Some(v) => {
                        f.write_str("=")?;
                        f.write_str(v)?;
                    }
                    None => {}
                }
            }
        }
        write_fragment(f, uri.fragment(), how.fragment)?;
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
        write_fragment(f, uri.fragment(), how.fragment)?;
        Ok(())
    }
}

impl fmt::Display for Redacted<'_, Uri> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.uri {
            Uri::Sip(u) => fmt::Display::fmt(&u.redacted(self.how), f),
            Uri::Tel(u) => fmt::Display::fmt(&u.redacted(self.how), f),
            Uri::Urn(u) => fmt::Display::fmt(&u.redacted(self.how), f),
            Uri::Other(o) => fmt::Display::fmt(&o.redacted(self.how), f),
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
            (OpaqueMask::Visible, _) => fmt::Display::fmt(self.uri, f),
            (OpaqueMask::Masked, Some(_))
                if self
                    .how
                    .is_public_urn(self.uri) =>
            {
                fmt::Display::fmt(self.uri, f)
            }
            (OpaqueMask::Masked, Some(nid))
                if self
                    .uri
                    .nss()
                    .is_none() =>
            {
                f.write_str("urn:")?;
                f.write_str(nid)
            }
            (OpaqueMask::Masked, Some(nid)) => {
                f.write_str("urn:")?;
                f.write_str(nid)?;
                f.write_str(":")?;
                f.write_str(MASK)
            }
            (OpaqueMask::Masked | OpaqueMask::Full, _) => {
                f.write_str("urn:")?;
                f.write_str(MASK)
            }
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
            (OpaqueMask::Visible, scheme) => {
                let raw = self
                    .uri
                    .as_str();
                let mut kept = 0;
                for (start, end) in other_password_spans(raw, scheme.map(str::len)) {
                    f.write_str(&raw[kept..start])?;
                    f.write_str(MASK)?;
                    kept = end;
                }
                f.write_str(&raw[kept..])
            }
            (OpaqueMask::Masked, Some(scheme)) => {
                let rest = self
                    .uri
                    .as_str()
                    .get(scheme.len() + 1..)
                    .unwrap_or_default();
                write_masked_other(
                    f,
                    scheme,
                    rest,
                    self.how
                        .fragment,
                )
            }
            (OpaqueMask::Masked | OpaqueMask::Full, _) => f.write_str(MASK),
        }
    }
}

/// Byte ranges of the passwords in an other URI's text, found as
/// sip-uri-types' `OtherUri` Debug finds them; see the module notes.
fn other_password_spans(raw: &str, scheme_end: Option<usize>) -> Vec<(usize, usize)> {
    let Some(hier) = scheme_end.and_then(|end| {
        raw.get(end + 1..)?
            .strip_prefix("//")
    }) else {
        return colon_password_spans(raw);
    };
    let authority_start = raw.len() - hier.len();
    let authority = hier
        .find(['/', '?', '#'])
        .map_or(hier, |end| &hier[..end]);
    let Some(at) = authority.rfind('@') else {
        return Vec::new();
    };
    authority[..at]
        .find(':')
        .map(|colon| (authority_start + colon + 1, authority_start + at))
        .into_iter()
        .collect()
}

/// In each run of text ending in `@`, whatever follows its second `:`.
fn colon_password_spans(raw: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    for (at, _) in raw.match_indices('@') {
        if let Some((colon, _)) = raw[start..at]
            .match_indices(':')
            .nth(1)
        {
            spans.push((start + colon + 1, at));
        }
        start = at + 1;
    }
    spans
}

/// `scheme://authority/path?query#fragment` with the userinfo and query
/// masked; a URI without `//` has no part to locate and is masked whole.
fn write_masked_other(
    f: &mut fmt::Formatter<'_>,
    scheme: &str,
    rest: &str,
    fragment_mask: FragmentMask,
) -> fmt::Result {
    let Some(hier) = rest.strip_prefix("//") else {
        f.write_str(scheme)?;
        f.write_str(":")?;
        return f.write_str(MASK);
    };
    let authority_end = hier
        .find(['/', '?', '#'])
        .unwrap_or(hier.len());
    let (authority, tail) = hier.split_at(authority_end);
    f.write_str(scheme)?;
    f.write_str("://")?;
    match authority.rsplit_once('@') {
        Some((_, host)) => {
            f.write_str(MASK)?;
            f.write_str("@")?;
            f.write_str(host)?;
        }
        None => f.write_str(authority)?,
    }
    let fragment_start = tail
        .find('#')
        .unwrap_or(tail.len());
    let (before_fragment, fragment) = tail.split_at(fragment_start);
    match before_fragment.split_once('?') {
        Some((path, _)) => {
            f.write_str(path)?;
            f.write_str("?")?;
            f.write_str(MASK)?;
        }
        None => f.write_str(before_fragment)?,
    }
    write_fragment(f, fragment.strip_prefix('#'), fragment_mask)
}

/// `#` and the fragment, or `#***`; nothing without one.
fn write_fragment(
    f: &mut fmt::Formatter<'_>,
    fragment: Option<&str>,
    mask: FragmentMask,
) -> fmt::Result {
    let Some(fragment) = fragment else {
        return Ok(());
    };
    f.write_str("#")?;
    match mask {
        FragmentMask::Visible => f.write_str(fragment),
        FragmentMask::Full => f.write_str(MASK),
    }
}
