use std::fmt;

use crate::warning::{ParseWarning, Parsed};

/// Error returned by every parser in this crate.
///
/// Lenient parsing fails only on [`Empty`](ParseError::Empty),
/// [`SchemeMismatch`](ParseError::SchemeMismatch) and, for a standalone
/// host, [`NoHost`](ParseError::NoHost); strict parsing also returns the
/// first grammar breach as [`NonConformant`](ParseError::NonConformant).
/// Display never quotes the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

// qual:allow(coupling, oi) reason: "into_strict needs ParseError; defined here, warning.rs stays free of error.rs"
impl<T> Parsed<T> {
    /// The value, or the first warning as [`ParseError::NonConformant`].
    pub fn into_strict(self) -> Result<T, ParseError> {
        match self
            .warnings
            .first()
        {
            Some(w) => Err(ParseError::NonConformant(*w)),
            None => Ok(self.value),
        }
    }
}
