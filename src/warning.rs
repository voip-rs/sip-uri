use sip_uri_diagnostics::{Component, ParseError, ParseWarning, WarningCode};

use crate::grammar::{validate_pct_encoded, CharClass};

/// A parse result together with the non-conformance found on the way.
///
/// Returned by [`UriParse::parse_with_warnings`](crate::UriParse::parse_with_warnings).
/// `value` is what [`UriParse::parse`](crate::UriParse::parse) returns for the
/// same input.
///
/// A plain result pair: callers destructure and build it, so its fields
/// are fixed, and anything added later is a method or another type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed<T> {
    /// The parsed value.
    pub value: T,
    /// Grammar breaches the parser accepted, in input order.
    pub warnings: Vec<ParseWarning>,
}

impl<T> Parsed<T> {
    /// `value` with the `warnings` found reading it.
    pub fn new(value: T, warnings: Vec<ParseWarning>) -> Self {
        Parsed { value, warnings }
    }

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

    /// The value, or the first warning as [`ParseError::NonConformant`].
    pub fn into_strict(self) -> Result<T, ParseError> {
        match self
            .warnings
            .into_iter()
            .next()
        {
            Some(w) => Err(ParseError::NonConformant(w)),
            None => Ok(self.value),
        }
    }
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
            .push(ParseWarning::new(component, code, position));
    }

    /// Warn on the first byte of `part` outside `allowed` or a well-formed escape.
    pub(crate) fn charset(&mut self, component: Component, part: &str, allowed: CharClass) {
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
