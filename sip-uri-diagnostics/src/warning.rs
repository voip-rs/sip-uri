//! Warnings: where a grammar breach is, what it is, and whether the value
//! survived it.
//!
//! A code's kebab-case name, description and kind are written once, in
//! the `warning_codes!` table and [`WarningCode::kind`]; a warning holds no
//! kind of its own, so it cannot disagree with its code.

use std::fmt;

/// A grammar breach the parser accepted rather than rejected.
///
/// Names where the breach is, never what text it was.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct ParseWarning {
    /// URI component the breach is in.
    pub component: Component,
    /// What is wrong with it.
    pub code: WarningCode,
    /// Byte offset into the parsed input, when one points at the breach.
    pub position: Option<usize>,
}

impl ParseWarning {
    /// A warning for `code` in `component`, at byte `position` of the parsed
    /// input when one points at the breach. The kind follows from the code.
    ///
    /// ```
    /// use sip_uri_diagnostics::{Component, ParseWarning, WarningCode, WarningKind};
    ///
    /// let w = ParseWarning::new(Component::Port, WarningCode::InvalidPort, None);
    /// assert_eq!(w.kind(), WarningKind::Lost);
    /// assert_eq!(w.code, WarningCode::InvalidPort);
    /// ```
    pub fn new(component: Component, code: WarningCode, position: Option<usize>) -> ParseWarning {
        ParseWarning {
            component,
            code,
            position,
        }
    }

    /// Whether the parsed value still carries what was sent: its code's kind.
    pub fn kind(&self) -> WarningKind {
        self.code
            .kind()
    }
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
    /// Every component, in declaration order.
    pub const ALL: &'static [Component] = &[
        Component::Scheme,
        Component::User,
        Component::UserParam,
        Component::Password,
        Component::Host,
        Component::Port,
        Component::Param,
        Component::Header,
        Component::Fragment,
        Component::Number,
        Component::RComponent,
        Component::QComponent,
        Component::FComponent,
        Component::Nid,
        Component::Nss,
        Component::Other,
    ];

    /// Stable kebab-case name, for logs and machine consumers.
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
    /// `*`, which is not a URI.
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
    /// A param, user-param or header that would take its list's text past
    /// `u32::MAX` bytes, dropped.
    TooLong => "too-long", "list text over the size limit dropped";
}

impl WarningCode {
    /// Stable kebab-case name, for logs and machine consumers.
    pub fn as_str(self) -> &'static str {
        self.names()
            .0
    }

    /// What the code found, in words for people.
    pub fn describe(self) -> &'static str {
        self.names()
            .1
    }

    /// Whether a value parsed past this breach still carries what was sent.
    pub fn kind(self) -> WarningKind {
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
            | WarningCode::MissingNss
            | WarningCode::TooLong => WarningKind::Lost,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParseError;

    #[test]
    fn every_code_has_a_distinct_kebab_case_name() {
        let mut names: Vec<&str> = WarningCode::ALL
            .iter()
            .map(|c| c.as_str())
            .collect();
        for name in &names {
            assert!(
                !name.is_empty()
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "{name:?}"
            );
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), WarningCode::ALL.len());
    }

    #[test]
    fn component_all_lists_every_variant_once() {
        let position = |c: Component| match c {
            Component::Scheme => 0,
            Component::User => 1,
            Component::UserParam => 2,
            Component::Password => 3,
            Component::Host => 4,
            Component::Port => 5,
            Component::Param => 6,
            Component::Header => 7,
            Component::Fragment => 8,
            Component::Number => 9,
            Component::RComponent => 10,
            Component::QComponent => 11,
            Component::FComponent => 12,
            Component::Nid => 13,
            Component::Nss => 14,
            Component::Other => 15,
        };
        let positions: Vec<usize> = Component::ALL
            .iter()
            .map(|&c| position(c))
            .collect();
        assert_eq!(positions, (0..=15).collect::<Vec<_>>());
    }

    #[test]
    fn every_component_has_a_distinct_kebab_case_name() {
        let mut names: Vec<&str> = Component::ALL
            .iter()
            .map(|c| c.as_str())
            .collect();
        assert!(names
            .iter()
            .all(|n| !n.is_empty()
                && n.bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'-')));
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Component::ALL.len());
    }

    /// Pins the layout; the wording itself is not part of the contract.
    #[test]
    fn display_is_component_description_and_position() {
        let w = ParseWarning::new(Component::User, WarningCode::InvalidChar, Some(5));
        assert_eq!(w.to_string(), "user: invalid character at byte 5");
        assert_eq!(
            ParseError::NonConformant(w).to_string(),
            "non-conformant URI: user: invalid character at byte 5"
        );
    }
}
