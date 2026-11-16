//! Rendering a URI for logs.
//!
//! What a log may carry is deployment policy, so every URI kind starts
//! masked and the caller relaxes each part on its own. A URN's NSS and the
//! text of an unrecognized URI are masked whole, since a device identifier
//! or a malformed caller URI lands there and no grammar here finds the
//! identity inside.

use std::fmt;
use std::sync::Arc;

use sip_uri_types::OtherUri;
use sip_uri_types::Pairs;
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
    /// A URN keeps its NID and becomes `urn:<nid>:***`; another URI keeps
    /// its scheme and becomes `<scheme>:***`, or `***` without one.
    Masked,
    /// Shown as Display writes it.
    Visible,
}

/// What a redacted rendering masks.
///
/// [`Redaction::default`] masks the whole userinfo, or a tel: number, and
/// every URI header value, since headers such as `P-Asserted-Identity`
/// carry identities: `sip:***@example.com?Subject=***`. It also masks a
/// URN's NSS and everything after the scheme of an unrecognized URI, where
/// device identifiers and malformed caller URIs land; see [`OpaqueMask`].
/// Params are shown unless named in [`Redaction::params`], except that an
/// `ext` or `isub` on a tel: URI or among SIP user-params, part of the
/// subscriber's number, is masked by the [`UserMask`] as the number is.
/// Routing params such as `rn` or `cic` stay shown. A password is `***` under every
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
}

impl Default for Redaction {
    fn default() -> Self {
        Redaction {
            user: UserMask::Full,
            headers: HeaderMask::Values,
            params: Arc::from([]),
            urn: OpaqueMask::Masked,
            other: OpaqueMask::Masked,
        }
    }
}

impl Redaction {
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

    /// Render the values of params with these names, compared
    /// case-insensitively, as `***`, in place of any set before.
    pub fn params<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.params = names
            .into_iter()
            .map(Into::into)
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

/// [`fmt::Display`] adapter returned by [`UriRedact::redacted`].
#[derive(Debug, Clone, Copy)]
pub struct Redacted<'a, T> {
    uri: &'a T,
    how: &'a Redaction,
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
        if let Some(host) = uri.host() {
            write!(f, "{host}")?;
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
            (_, Some(nid)) => write!(f, "urn:{nid}:{MASK}"),
            (_, None) => write!(f, "urn:{MASK}"),
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
            (_, Some(scheme)) => write!(f, "{scheme}:{MASK}"),
            (_, None) => f.write_str(MASK),
        }
    }
}
