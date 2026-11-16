//! Serde adapters that write a URI or host as its [`Display`] text and read
//! it back with the lenient parser.
//!
//! Use one with `#[serde(with = …)]`, or its `option` submodule for an
//! `Option` field. A read that fails reports the [`ParseError`], or the type
//! it expected, never the value it read. A `SipUri` built with no component
//! writes as `""`, which reads back as [`ParseError::Empty`].
//!
//! ```
//! # use serde::{Deserialize, Serialize};
//! use sip_uri::{SipUri, Uri};
//!
//! #[derive(Serialize, Deserialize)]
//! struct Call {
//!     #[serde(with = "sip_uri::serde_str::uri")]
//!     to: Uri,
//!     #[serde(with = "sip_uri::serde_str::sip_uri::option", default)]
//!     contact: Option<SipUri>,
//! }
//!
//! let call: Call = serde_json::from_str(r#"{"to": "tel:+15551234567"}"#).unwrap();
//! assert_eq!(serde_json::to_string(&call).unwrap(), r#"{"to":"tel:+15551234567","contact":null}"#);
//! ```

use std::fmt::{self, Display};
use std::marker::PhantomData;

use serde::de::{Error, Visitor};
use serde::{Deserializer, Serialize, Serializer};

use crate::{ParseError, UriParse};

fn serialize<T: Display, S: Serializer>(value: &T, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

/// A read's outcome: the deserializer's own error, or the parse result.
type Read<T, E> = Result<Result<T, ParseError>, E>;

/// The value read, an error naming only what was `expected` when the
/// deserializer failed, or the parse error.
fn read_text<T, E: Error>(expected: &str, read: Read<T, E>) -> Result<T, E> {
    // The error is dropped unread: serde's messages quote the value.
    read.map_err(|_| E::custom(format_args!("invalid type: expected {expected}")))?
        .map_err(E::custom)
}

struct TextVisitor<T>(PhantomData<T>);

impl<T: UriParse> Visitor<'_> for TextVisitor<T> {
    type Value = Result<T, ParseError>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("URI text")
    }

    fn visit_str<E: Error>(self, text: &str) -> Result<Self::Value, E> {
        Ok(T::parse(text))
    }
}

struct OptionVisitor<T>(PhantomData<T>);

impl<'de, T: UriParse> Visitor<'de> for OptionVisitor<T> {
    type Value = Result<Option<T>, ParseError>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("URI text or null")
    }

    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        Ok(Ok(None))
    }

    fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
        Ok(Ok(None))
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Read<Option<T>, D::Error> {
        deserializer
            .deserialize_str(TextVisitor(PhantomData))
            .map(|parsed| parsed.map(Some))
    }
}

fn deserialize<'de, T: UriParse, D: Deserializer<'de>>(deserializer: D) -> Result<T, D::Error> {
    read_text(
        "URI text",
        deserializer.deserialize_str(TextVisitor(PhantomData)),
    )
}

struct AsText<'a, T>(&'a T);

impl<T: Display> Serialize for AsText<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self.0)
    }
}

fn serialize_option<T: Display, S: Serializer>(
    value: &Option<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(v) => serializer.serialize_some(&AsText(v)),
        None => serializer.serialize_none(),
    }
}

fn deserialize_option<'de, T: UriParse, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    read_text(
        "URI text or null",
        deserializer.deserialize_option(OptionVisitor(PhantomData)),
    )
}

macro_rules! adapter {
    ($($(#[$doc:meta])* $name:ident => $ty:ty;)*) => {$(
        $(#[$doc])*
        pub mod $name {
            use serde::{Deserializer, Serializer};

            /// Write the value as its `Display` text.
            pub fn serialize<S: Serializer>(value: &$ty, serializer: S) -> Result<S::Ok, S::Error> {
                super::serialize(value, serializer)
            }

            /// Read the value with the lenient parser.
            pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<$ty, D::Error> {
                super::deserialize(deserializer)
            }

            /// The same adapter for an `Option` field, `None` as null.
            pub mod option {
                use serde::{Deserializer, Serializer};

                /// Write the value as its `Display` text, or null.
                pub fn serialize<S: Serializer>(
                    value: &Option<$ty>,
                    serializer: S,
                ) -> Result<S::Ok, S::Error> {
                    super::super::serialize_option(value, serializer)
                }

                /// Read the value with the lenient parser, null as `None`.
                pub fn deserialize<'de, D: Deserializer<'de>>(
                    deserializer: D,
                ) -> Result<Option<$ty>, D::Error> {
                    super::super::deserialize_option(deserializer)
                }
            }
        }
    )*};
}

adapter! {
    /// [`Uri`](crate::Uri) as text.
    uri => crate::Uri;
    /// [`SipUri`](crate::SipUri) as text.
    sip_uri => crate::SipUri;
    /// [`TelUri`](crate::TelUri) as text.
    tel_uri => crate::TelUri;
    /// [`UrnUri`](crate::UrnUri) as text.
    urn_uri => crate::UrnUri;
    /// [`Host`](crate::Host) as text, IPv6 bracketed.
    host => crate::Host;
}
