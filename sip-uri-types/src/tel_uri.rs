use std::borrow::Cow;
use std::fmt;

use crate::canon;
use crate::params::{self, Params};

/// tel: URI per RFC 3966.
///
/// Parse text into one with sip-uri's `UriParse`.
///
/// Represents a telephone number with optional parameters.
/// Global numbers start with `+`. A local number without the `phone-context`
/// parameter RFC 3966 requires is still a value, and a missing number is
/// `None`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(from = "TelUriParts", into = "TelUriParts")
)]
#[non_exhaustive]
pub struct TelUri {
    number: Option<String>,
    params: Params,
    fragment: Option<String>,
}

/// The components of a [`TelUri`], each canonized on conversion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub struct TelUriParts {
    /// The telephone number, `+` first for a global one.
    pub number: Option<String>,
    /// Parameters after the number.
    pub params: Params,
    /// The fragment after `#`.
    pub fragment: Option<String>,
}

impl From<TelUriParts> for TelUri {
    fn from(p: TelUriParts) -> Self {
        TelUri {
            number: p
                .number
                .and_then(hold_number),
            params: p.params,
            fragment: p
                .fragment
                .and_then(hold_fragment),
        }
    }
}

fn hold_number<'a>(number: impl Into<Cow<'a, str>>) -> Option<String> {
    let number = number.into();
    (!number.is_empty()).then(|| canon::canonize_tel_number(number))
}

fn hold_fragment<'a>(fragment: impl Into<Cow<'a, str>>) -> Option<String> {
    let fragment = fragment.into();
    (!fragment.is_empty()).then(|| canon::canonize_fragment(fragment))
}

impl From<TelUri> for TelUriParts {
    fn from(uri: TelUri) -> Self {
        uri.into_parts()
    }
}

impl TelUri {
    /// Create a new tel: URI with the given number, `+` first for a global
    /// one. A delimiter or byte outside the number grammar is escaped.
    pub fn new<'a>(number: impl Into<Cow<'a, str>>) -> Self {
        TelUri {
            number: hold_number(number.into()),
            params: Params::new(),
            fragment: None,
        }
    }

    /// The components, in canonical form.
    pub fn into_parts(self) -> TelUriParts {
        TelUriParts {
            number: self.number,
            params: self.params,
            fragment: self.fragment,
        }
    }

    /// Add a parameter, escaping any delimiter in the name or value.
    pub fn with_param(mut self, name: impl AsRef<str>, value: Option<&str>) -> Self {
        self.params
            .push(name, value);
        self
    }

    /// The telephone number (including `+` for global numbers, including
    /// visual separators), `None` when the input had none.
    pub fn number(&self) -> Option<&str> {
        self.number
            .as_deref()
    }

    /// Whether this is a global number (starts with `+`).
    pub fn is_global(&self) -> bool {
        self.number
            .as_deref()
            .is_some_and(|n| n.starts_with('+'))
    }

    /// Parameters.
    pub fn params(&self) -> &Params {
        &self.params
    }

    /// The parameters, to edit in place; every insertion canonizes.
    pub fn params_mut(&mut self) -> &mut Params {
        &mut self.params
    }

    /// Look up a parameter by name (case-insensitive): `Some(None)` when it
    /// has no value.
    pub fn param(&self, name: &str) -> Option<Option<&str>> {
        self.params
            .get(name)
    }

    /// The fragment component (after `#`), if present.
    pub fn fragment(&self) -> Option<&str> {
        self.fragment
            .as_deref()
    }

    /// Set the fragment component, escaping any delimiter in it.
    pub fn with_fragment<'a>(mut self, fragment: impl Into<Cow<'a, str>>) -> Self {
        self.fragment = hold_fragment(fragment.into());
        self
    }
}

impl fmt::Display for TelUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("tel:")?;
        if let Some(ref number) = self.number {
            f.write_str(number)?;
        }
        params::format_params(
            self.params
                .iter(),
            f,
        )?;
        if let Some(ref frag) = self.fragment {
            f.write_str("#")?;
            f.write_str(frag)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder() {
        let uri = TelUri::new("+15551234567").with_param("cpc", Some("emergency"));
        assert_eq!(uri.to_string(), "tel:+15551234567;cpc=emergency");
    }

    #[test]
    fn builders_hold_an_owned_canonical_string() {
        let number = String::from("+15551234567");
        let fragment = String::from("f");
        let buffers = [number.as_ptr(), fragment.as_ptr()].map(Some);
        let uri = TelUri::new(number).with_fragment(fragment);
        let held = [uri.number(), uri.fragment()].map(|c| c.map(str::as_ptr));
        assert_eq!(held, buffers);
    }

    #[test]
    fn parts_canonize_like_builders() {
        let uri = TelUri::from(TelUriParts {
            number: Some("+1555 123".into()),
            params: Params::new().with("a;b", None),
            ..Default::default()
        });
        assert_eq!(uri, TelUri::new("+1555 123").with_param("a;b", None));
        assert_eq!(uri.number(), Some("+1555%20123"));
        assert_eq!(
            TelUri::from(
                uri.clone()
                    .into_parts()
            ),
            uri
        );
    }

    #[test]
    fn empty_components_are_absent() {
        let uri = TelUri::new("")
            .with_param("", None)
            .with_fragment("");
        assert_eq!(uri, TelUri::from(TelUriParts::default()));
        assert_eq!(uri.number(), None);
        assert_eq!(uri.to_string(), "tel:");
    }
}
