//! The one error every sip-uri parser returns.

use std::fmt;

use crate::ParseWarning;

/// Error returned by every sip-uri parser.
///
/// Lenient parsing fails only on [`Empty`](ParseError::Empty),
/// [`SchemeMismatch`](ParseError::SchemeMismatch) and, for a standalone
/// host, [`NoHost`](ParseError::NoHost); strict parsing also returns the
/// first grammar breach as [`NonConformant`](ParseError::NonConformant).
/// Display never quotes the input.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParseError {
    /// The input is empty, or a SIP URI's input leaves no component, such
    /// as `:` or `@` alone.
    Empty,
    /// The input's scheme names another kind of URI.
    SchemeMismatch,
    /// A standalone host's input holds no readable host.
    NoHost,
    /// A strict parse met a grammar breach.
    NonConformant(ParseWarning),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => f.write_str("nothing to read"),
            ParseError::SchemeMismatch => f.write_str("scheme belongs to another URI type"),
            ParseError::NoHost => f.write_str("no readable host"),
            ParseError::NonConformant(w) => write!(f, "non-conformant URI: {w}"),
        }
    }
}

impl std::error::Error for ParseError {}
