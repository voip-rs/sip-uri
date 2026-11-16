use std::fmt;

use crate::grammar::validate_pct_encoded;

/// A parse result together with the non-conformance found on the way.
///
/// Returned by [`UriParse::parse_with_warnings`](crate::UriParse::parse_with_warnings).
/// `value` is what [`UriParse::parse`](crate::UriParse::parse) returns for the
/// same input.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Parsed<T> {
    /// The parsed value.
    pub value: T,
    /// Grammar breaches the parser accepted, in input order.
    pub warnings: Vec<ParseWarning>,
}

impl<T> Parsed<T> {
    /// Whether any warning was raised.
    pub fn has_warnings(&self) -> bool {
        !self
            .warnings
            .is_empty()
    }

    /// Apply `f` to the value, keeping the warnings.
    ///
    /// ```
    /// use sip_uri::{SipUri, Uri, UriParse};
    ///
    /// let parsed = SipUri::parse_with_warnings("sip:example.com:+5060").unwrap();
    /// let as_uri = parsed.map(Uri::Sip);
    /// assert!(as_uri.value.as_sip().is_some());
    /// assert!(as_uri.has_warnings());
    /// ```
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Parsed<U> {
        Parsed {
            value: f(self.value),
            warnings: self.warnings,
        }
    }
}

/// A grammar breach the parser accepted rather than rejected.
///
/// Names where the breach is, never what text it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ParseWarning {
    /// URI component the breach is in.
    pub component: Component,
    /// What is wrong with it.
    pub code: WarningCode,
    /// Byte offset into the parsed input, when one points at the breach.
    pub position: Option<usize>,
    /// Whether the parsed value still carries what was sent.
    pub kind: WarningKind,
}

impl fmt::Display for ParseWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}",
            self.component
                .as_str(),
            self.code
                .describe()
        )?;
        if let Some(pos) = self.position {
            write!(f, " at byte {pos}")?;
        }
        Ok(())
    }
}

/// URI component a [`ParseWarning`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Component {
    /// The scheme before the first `:`.
    Scheme,
    /// SIP user part.
    User,
    /// SIP user-param, inside the userinfo.
    UserParam,
    /// SIP password.
    Password,
    /// Host.
    Host,
    /// Port.
    Port,
    /// URI parameter after the host, or a tel: parameter.
    Param,
    /// SIP URI header after `?`.
    Header,
    /// Fragment after `#`.
    Fragment,
    /// tel: telephone number.
    Number,
    /// URN r-component (`?+`).
    RComponent,
    /// URN q-component (`?=`).
    QComponent,
    /// URN f-component (`#`).
    FComponent,
    /// URN namespace identifier.
    Nid,
    /// URN namespace-specific string.
    Nss,
    /// Text after the scheme of a URI whose scheme has no type here.
    Other,
}

impl Component {
    /// Stable lowercase name, for logs and machine consumers.
    pub fn as_str(self) -> &'static str {
        match self {
            Component::Scheme => "scheme",
            Component::User => "user",
            Component::UserParam => "user-param",
            Component::Password => "password",
            Component::Host => "host",
            Component::Port => "port",
            Component::Param => "param",
            Component::Header => "header",
            Component::Fragment => "fragment",
            Component::Number => "number",
            Component::Nid => "nid",
            Component::Nss => "nss",
            Component::RComponent => "r-component",
            Component::QComponent => "q-component",
            Component::FComponent => "f-component",
            Component::Other => "other",
        }
    }
}

/// One row per code: its docs, variant, kebab-case name and description.
macro_rules! warning_codes {
    ($($(#[$doc:meta])* $variant:ident => $name:literal, $description:literal;)*) => {
        /// What a [`ParseWarning`] found.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum WarningCode {
            $($(#[$doc])* $variant,)*
        }

        impl WarningCode {
            /// Every code, in declaration order.
            pub const ALL: &'static [WarningCode] = &[$(WarningCode::$variant),*];

            fn names(self) -> (&'static str, &'static str) {
                match self {
                    $(WarningCode::$variant => ($name, $description),)*
                }
            }
        }
    };
}

warning_codes! {
    /// A character outside the component's character set, held as its
    /// escaped octet. In [`Component::Other`] only a byte that would break a
    /// header line is escaped; any other is held as sent.
    InvalidChar => "invalid-char", "invalid character";
    /// A `%` not followed by two hex digits, held as the octet `%`, `%25`,
    /// except in [`Component::Other`], which holds it as sent.
    MalformedEscape => "malformed-escape", "malformed percent-escape";
    /// A parameter with no name before `=`.
    EmptyName => "empty-name", "empty parameter name";
    /// An empty `;` or `&` segment, dropped.
    EmptySegment => "empty-segment", "empty segment dropped";
    /// A password with no user before it.
    PasswordWithoutUser => "password-without-user", "password without user";
    /// A port written with a `+` sign.
    SignedPort => "signed-port", "signed port";
    /// A `:` with no port after it, dropped.
    EmptyPort => "empty-port", "empty port dropped";
    /// A host label that is empty or starts or ends with `-`.
    InvalidHostLabel => "invalid-host-label", "invalid host label";
    /// A hostname whose last label does not start with a letter.
    NumericToplabel => "numeric-toplabel", "last host label does not start with a letter";
    /// A hostname written with percent-escapes.
    EscapedHost => "escaped-host", "percent-escaped hostname";
    /// A fragment on a URI whose grammar defines none.
    UnexpectedFragment => "unexpected-fragment", "fragment not defined for this scheme";
    /// A `#` with nothing after it, dropped.
    EmptyFragment => "empty-fragment", "empty fragment dropped";
    /// A user part containing `?`, likely a header value whose `@` was taken
    /// as the userinfo delimiter.
    HeaderShapedUser => "header-shaped-user", "user part contains '?'";
    /// A local tel: number without `phone-context`.
    MissingPhoneContext => "missing-phone-context", "local number without phone-context";
    /// A URN `?+` or `?=` with nothing after it.
    EmptyComponent => "empty-component", "empty component";
    /// A scheme outside the RFC 3986 grammar.
    InvalidScheme => "invalid-scheme", "invalid scheme";
    /// No scheme before the rest of the URI.
    MissingScheme => "missing-scheme", "missing scheme";
    /// `*`, which is not a URI but the Contact and OPTIONS wildcard.
    Wildcard => "wildcard", "wildcard is not a URI";
    /// No host where the grammar requires one.
    MissingHost => "missing-host", "missing host";
    /// A bracketed IPv6 reference that is unclosed or not an IPv6 address.
    InvalidIpv6 => "invalid-ipv6", "invalid IPv6 reference dropped";
    /// A port that is not a number in 0-65535, dropped.
    InvalidPort => "invalid-port", "invalid port dropped";
    /// Text after the host or port that starts no component, dropped.
    TrailingContent => "trailing-content", "trailing content dropped";
    /// A URI header with no `=`, held without a value.
    MissingValue => "missing-value", "header without '='";
    /// An empty user before user-params.
    EmptyUser => "empty-user", "empty user";
    /// An `@` with no userinfo before it, dropped.
    EmptyUserinfo => "empty-userinfo", "empty userinfo dropped";
    /// A tel: URI with no number.
    MissingNumber => "missing-number", "missing number";
    /// A tel: number with no digit.
    NoDigits => "no-digits", "number without digits";
    /// A URN with no namespace identifier.
    MissingNid => "missing-nid", "missing namespace identifier";
    /// A URN with no namespace-specific string.
    MissingNss => "missing-nss", "missing namespace-specific string";
    /// A URN namespace identifier outside RFC 8141's length or character rules.
    InvalidNid => "invalid-nid", "invalid namespace identifier";
    /// An IPv6 zone identifier after `%`, which a host cannot hold, dropped.
    Ipv6Zone => "ipv6-zone", "IPv6 zone identifier dropped";
}

impl WarningCode {
    /// Stable kebab-case name, for logs and machine consumers.
    pub fn as_str(self) -> &'static str {
        self.names()
            .0
    }

    fn describe(self) -> &'static str {
        self.names()
            .1
    }

    fn kind(self) -> WarningKind {
        match self {
            WarningCode::EmptySegment
            | WarningCode::EmptyPort
            | WarningCode::EmptyFragment
            | WarningCode::MissingScheme
            | WarningCode::MissingHost
            | WarningCode::InvalidIpv6
            | WarningCode::Ipv6Zone
            | WarningCode::InvalidPort
            | WarningCode::TrailingContent
            | WarningCode::EmptyUserinfo
            | WarningCode::MissingNumber
            | WarningCode::MissingNid
            | WarningCode::MissingNss => WarningKind::Lost,
            _ => WarningKind::Recovered,
        }
    }
}

/// Whether the parsed value still carries what the input sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WarningKind {
    /// Kept; re-serializing carries the same octets, canonically escaped.
    Recovered,
    /// Dropped; re-serializing omits it, except a tel: or URN scheme, which
    /// the type implies and prints.
    Lost,
}

/// Collects warnings with positions relative to the whole parsed input.
pub(crate) struct Warnings<'a> {
    input: &'a str,
    list: Vec<ParseWarning>,
}

impl<'a> Warnings<'a> {
    pub(crate) fn new(input: &'a str) -> Self {
        Warnings {
            input,
            list: Vec::new(),
        }
    }

    /// Record `code` at byte `at` of `part`, a slice borrowed from the input.
    pub(crate) fn push(&mut self, component: Component, code: WarningCode, part: &str, at: usize) {
        let position = (part.as_ptr() as usize)
            .checked_sub(
                self.input
                    .as_ptr() as usize,
            )
            .map(|start| start + at)
            .filter(|&pos| {
                pos <= self
                    .input
                    .len()
            });
        self.list
            .push(ParseWarning {
                component,
                code,
                position,
                kind: code.kind(),
            });
    }

    /// Warn on the first byte of `part` outside `allowed` or a well-formed escape.
    pub(crate) fn charset(&mut self, component: Component, part: &str, allowed: fn(u8) -> bool) {
        if let Err(pos) = validate_pct_encoded(part, allowed) {
            let code = if part.as_bytes()[pos] == b'%' {
                WarningCode::MalformedEscape
            } else {
                WarningCode::InvalidChar
            };
            self.push(component, code, part, pos);
        }
    }

    /// The warnings in input order: parsers check components in whatever
    /// order suits them, and a warning without a position goes last.
    pub(crate) fn finish<T>(mut self, value: T) -> Parsed<T> {
        self.list
            .sort_by_key(|w| {
                (
                    w.position
                        .is_none(),
                    w.position,
                )
            });
        Parsed {
            value,
            warnings: self.list,
        }
    }
}
