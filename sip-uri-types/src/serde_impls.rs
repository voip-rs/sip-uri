use std::cell::Cell;
use std::fmt;
use std::net::{Ipv4Addr, Ipv6Addr};

use serde::de::{
    DeserializeSeed, EnumAccess, Error as _, IgnoredAny, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::host::{Host, Hostname};
use crate::params::{CapacityError, Headers, Params, UserParams};
use crate::sip_uri::{Scheme, SipUriParts};
use crate::tel_uri::TelUriParts;
use crate::uri::{OtherUri, Uri};
use crate::urn_uri::UrnUriParts;
use crate::{SipUri, TelUri, UrnUri};

macro_rules! pair_list_serde {
    ($($ty:ty => $label:literal),*) => {$(
        impl Serialize for $ty {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_seq(self.iter())
            }
        }

        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                shaped(
                    deserializer,
                    concat!("`", $label, "`"),
                    "a sequence of [name, value] pairs",
                    |d| {
                        d.deserialize_seq(PairsVisitor {
                            label: $label,
                            push: |list: &mut $ty, name, value| list.try_push(name, value),
                        })
                    },
                )
            }
        }
    )*};
}

pair_list_serde!(Params => "params", UserParams => "user_params", Headers => "headers");

/// Collects a list named `label` through its fallible `push`.
struct PairsVisitor<L> {
    label: &'static str,
    push: fn(&mut L, &str, Option<&str>) -> Result<(), CapacityError>,
}

impl<'de, L: Default> Visitor<'de> for PairsVisitor<L> {
    type Value = L;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a sequence of [name, value] pairs")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut list = L::default();
        while let Some((name, value)) = seq.next_element_seed(PairSeed(self.label))? {
            (self.push)(&mut list, &name, value.as_deref())
                .map_err(|e| A::Error::custom(format_args!("invalid `{}`: {e}", self.label)))?;
        }
        Ok(list)
    }
}

struct PairSeed(&'static str);

impl<'de> DeserializeSeed<'de> for PairSeed {
    type Value = (String, Option<String>);

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        shaped(
            deserializer,
            format_args!("`{}` entry", self.0),
            "a [name, value] pair",
            |d| d.deserialize_tuple(2, PairVisitor(self.0)),
        )
    }
}

struct PairVisitor(&'static str);

impl<'de> Visitor<'de> for PairVisitor {
    type Value = (String, Option<String>);

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a [name, value] pair")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let label = self.0;
        let invalid = |part: &str, expected: &str| {
            A::Error::custom(format_args!(
                "invalid `{label}` {part}: expected {expected}"
            ))
        };
        let short = || invalid("entry", "a [name, value] pair");
        let name = seq
            .next_element::<String>()
            .map_err(|_| invalid("name", "a string"))?
            .ok_or_else(short)?;
        let value = seq
            .next_element::<Option<String>>()
            .map_err(|_| invalid("value", "a string or null"))?
            .ok_or_else(short)?;
        match seq.next_element::<IgnoredAny>() {
            Ok(None) => Ok((name, value)),
            _ => Err(short()),
        }
    }
}

/// The serialized shape of [`Host`], read before [`Host::from_hostname`].
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum HostRepr {
    #[serde(deserialize_with = "field::ipv4")]
    Ipv4(Ipv4Addr),
    #[serde(deserialize_with = "field::ipv6")]
    Ipv6(Ipv6Addr),
    Hostname(Hostname),
}

impl<'de> Deserialize<'de> for Host {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = shaped(
            deserializer,
            "host",
            "a map with one of ipv4, ipv6 or hostname",
            |d| HostRepr::deserialize(d),
        )?;
        Ok(match repr {
            HostRepr::Ipv4(addr) => Host::Ipv4(addr),
            HostRepr::Ipv6(addr) => Host::Ipv6(addr),
            HostRepr::Hostname(name) => Host::from_hostname(name),
        })
    }
}

impl Serialize for Hostname {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Hostname {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        leaf(deserializer, "hostname", "a string", String::deserialize).map(Hostname::from)
    }
}

#[derive(Deserialize)]
#[serde(remote = "Scheme", rename_all = "lowercase")]
enum SchemeDef {
    Sip,
    Sips,
}

impl<'de> Deserialize<'de> for Scheme {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        leaf(
            deserializer,
            "scheme",
            "sip or sips",
            SchemeDef::deserialize,
        )
    }
}

#[derive(Deserialize)]
#[serde(remote = "Uri", rename_all = "lowercase")]
enum UriDef {
    Sip(SipUri),
    Tel(TelUri),
    Urn(UrnUri),
    Other(OtherUri),
}

impl<'de> Deserialize<'de> for Uri {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        shaped(
            deserializer,
            "URI",
            "a map with one of sip, tel, urn or other",
            |d| UriDef::deserialize(d),
        )
    }
}

#[derive(Deserialize, Default)]
#[serde(remote = "SipUriParts", default)]
struct SipUriPartsDef {
    #[serde(deserialize_with = "field::scheme")]
    scheme: Option<Scheme>,
    #[serde(deserialize_with = "field::user")]
    user: Option<String>,
    user_params: UserParams,
    #[serde(deserialize_with = "field::password")]
    password: Option<String>,
    host: Option<Host>,
    #[serde(deserialize_with = "field::port")]
    port: Option<u16>,
    params: Params,
    headers: Headers,
    #[serde(deserialize_with = "field::fragment")]
    fragment: Option<String>,
}

impl<'de> Deserialize<'de> for SipUriParts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        shaped(deserializer, "SIP URI", "a map", |d| {
            SipUriPartsDef::deserialize(d)
        })
    }
}

#[derive(Deserialize, Default)]
#[serde(remote = "TelUriParts", default)]
struct TelUriPartsDef {
    #[serde(deserialize_with = "field::number")]
    number: Option<String>,
    params: Params,
    #[serde(deserialize_with = "field::fragment")]
    fragment: Option<String>,
}

impl<'de> Deserialize<'de> for TelUriParts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        shaped(deserializer, "tel URI", "a map", |d| {
            TelUriPartsDef::deserialize(d)
        })
    }
}

#[derive(Deserialize, Default)]
#[serde(remote = "UrnUriParts", default)]
struct UrnUriPartsDef {
    #[serde(deserialize_with = "field::nid")]
    nid: Option<String>,
    #[serde(deserialize_with = "field::nss")]
    nss: Option<String>,
    #[serde(deserialize_with = "field::r_component")]
    r_component: Option<String>,
    #[serde(deserialize_with = "field::q_component")]
    q_component: Option<String>,
    #[serde(deserialize_with = "field::f_component")]
    f_component: Option<String>,
}

impl<'de> Deserialize<'de> for UrnUriParts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        shaped(deserializer, "URN", "a map", |d| {
            UrnUriPartsDef::deserialize(d)
        })
    }
}

#[derive(Serialize)]
struct OtherRef<'a> {
    scheme: Option<&'a str>,
    rest: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct OtherOwned {
    #[serde(deserialize_with = "field::scheme")]
    scheme: Option<String>,
    #[serde(deserialize_with = "field::rest")]
    rest: String,
}

impl Serialize for OtherUri {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OtherRef {
            scheme: self.scheme(),
            rest: self.rest(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OtherUri {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let other = shaped(deserializer, "other URI", "a map", |d| {
            OtherOwned::deserialize(d)
        })?;
        OtherUri::new(
            other
                .scheme
                .as_deref(),
            other.rest,
        )
        .map_err(D::Error::custom)
    }
}

/// What a field reads, in words, for the error naming it.
trait Expected {
    const TEXT: &'static str;
}

impl Expected for String {
    const TEXT: &'static str = "a string";
}

impl Expected for Option<String> {
    const TEXT: &'static str = "a string or null";
}

impl Expected for Option<u16> {
    const TEXT: &'static str = "a port number or null";
}

impl Expected for Option<Scheme> {
    const TEXT: &'static str = "sip, sips or null";
}

impl Expected for Ipv4Addr {
    const TEXT: &'static str = "an IPv4 address";
}

impl Expected for Ipv6Addr {
    const TEXT: &'static str = "an IPv6 address";
}

/// `read` over a value where `what` is expected, described by `expected`;
/// any error becomes one naming both.
fn leaf<'de, D: Deserializer<'de>, T>(
    deserializer: D,
    what: &str,
    expected: &str,
    read: impl FnOnce(D) -> Result<T, D::Error>,
) -> Result<T, D::Error> {
    // The error is dropped unread: serde's messages quote the value.
    read(deserializer)
        .map_err(|_| D::Error::custom(format_args!("invalid {what}: expected {expected}")))
}

/// `read` over a value where `what` is expected, described by `expected`;
/// errors raised before its map, sequence or enum variant's content opens
/// become one naming both, later ones are its fields' own.
fn shaped<'de, D: Deserializer<'de>, T>(
    deserializer: D,
    what: impl fmt::Display,
    expected: &str,
    read: impl FnOnce(Shaped<'_, D>) -> Result<T, D::Error>,
) -> Result<T, D::Error> {
    let opened = Cell::new(false);
    read(Shaped {
        inner: deserializer,
        opened: &opened,
    })
    .map_err(|e| {
        if opened.get() {
            e
        } else {
            D::Error::custom(format_args!("invalid {what}: expected {expected}"))
        }
    })
}

/// Field adapters for `deserialize_with`, each a [`leaf`] named for its field.
mod field {
    use serde::{Deserialize, Deserializer};

    use super::Expected;

    macro_rules! fields {
        ($($name:ident)*) => {$(
            pub(super) fn $name<'de, T: Deserialize<'de> + Expected, D: Deserializer<'de>>(
                deserializer: D,
            ) -> Result<T, D::Error> {
                super::leaf(
                    deserializer,
                    concat!("`", stringify!($name), "`"),
                    T::TEXT,
                    T::deserialize,
                )
            }
        )*};
    }

    fields! {
        f_component fragment ipv4 ipv6 nid nss number password port q_component r_component
        rest scheme user
    }
}

/// A deserializer marking `opened` once its value's map, sequence or enum
/// variant's content opens.
struct Shaped<'a, D> {
    inner: D,
    opened: &'a Cell<bool>,
}

impl<'a, D> Shaped<'a, D> {
    fn opens<V>(&self, visitor: V) -> Opens<'a, V> {
        Opens {
            inner: visitor,
            opened: self.opened,
        }
    }
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Shaped<'_, D> {
    type Error = D::Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
        let visitor = self.opens(visitor);
        self.inner
            .deserialize_any(visitor)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
        let visitor = self.opens(visitor);
        self.inner
            .deserialize_seq(visitor)
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.opens(visitor);
        self.inner
            .deserialize_tuple(len, visitor)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.opens(visitor);
        self.inner
            .deserialize_struct(name, fields, visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, D::Error> {
        let visitor = self.opens(visitor);
        self.inner
            .deserialize_enum(name, variants, visitor)
    }

    fn is_human_readable(&self) -> bool {
        self.inner
            .is_human_readable()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct tuple_struct map
        identifier ignored_any
    }
}

/// A visitor marking `opened` before it reads a map or sequence, or an enum
/// variant's content.
struct Opens<'a, V> {
    inner: V,
    opened: &'a Cell<bool>,
}

impl<'de, V: Visitor<'de>> Visitor<'de> for Opens<'_, V> {
    type Value = V::Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner
            .expecting(f)
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<V::Value, A::Error> {
        self.opened
            .set(true);
        self.inner
            .visit_map(map)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<V::Value, A::Error> {
        self.opened
            .set(true);
        self.inner
            .visit_seq(seq)
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<V::Value, A::Error> {
        self.inner
            .visit_enum(OpensVariant {
                inner: data,
                opened: self.opened,
            })
    }
}

struct OpensVariant<'a, A> {
    inner: A,
    opened: &'a Cell<bool>,
}

impl<'de, 'a, A: EnumAccess<'de>> EnumAccess<'de> for OpensVariant<'a, A> {
    type Error = A::Error;
    type Variant = OpensVariant<'a, A::Variant>;

    fn variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> Result<(S::Value, Self::Variant), A::Error> {
        let (value, variant) = self
            .inner
            .variant_seed(seed)?;
        Ok((
            value,
            OpensVariant {
                inner: variant,
                opened: self.opened,
            },
        ))
    }
}

impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for OpensVariant<'_, A> {
    type Error = A::Error;

    fn unit_variant(self) -> Result<(), A::Error> {
        self.inner
            .unit_variant()
    }

    fn newtype_variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, A::Error> {
        self.inner
            .newtype_variant_seed(OpensContent {
                inner: seed,
                opened: self.opened,
            })
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, A::Error> {
        self.inner
            .tuple_variant(
                len,
                Opens {
                    inner: visitor,
                    opened: self.opened,
                },
            )
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, A::Error> {
        self.inner
            .struct_variant(
                fields,
                Opens {
                    inner: visitor,
                    opened: self.opened,
                },
            )
    }
}

/// A seed marking `opened` once an enum variant's content is read.
struct OpensContent<'a, S> {
    inner: S,
    opened: &'a Cell<bool>,
}

impl<'de, S: DeserializeSeed<'de>> DeserializeSeed<'de> for OpensContent<'_, S> {
    type Value = S::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<S::Value, D::Error> {
        self.opened
            .set(true);
        self.inner
            .deserialize(deserializer)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        Headers, Host, Hostname, OtherUri, Params, Scheme, SipUri, SipUriParts, TelUri,
        TelUriParts, Uri, UrnUri, UrnUriParts, UserParams,
    };
    use serde::de::DeserializeOwned;
    use serde::Serialize;
    use serde_json::{json, Value};

    fn round_trip(uri: Uri) {
        let json = serde_json::to_value(&uri).unwrap();
        assert_eq!(serde_json::from_value::<Uri>(json).unwrap(), uri);
    }

    #[test]
    fn sip_uri_serializes_as_parts() {
        let uri = SipUri::new(Host::Hostname("example.com".into()))
            .with_user("+15551234567")
            .with_param("user", Some("phone"))
            .with_header("Subject", Some("x"))
            .with_header("Flag", None);
        assert_eq!(
            serde_json::to_value(&uri).unwrap(),
            json!({
                "scheme": "sip",
                "user": "+15551234567",
                "user_params": [],
                "password": null,
                "host": {"hostname": "example.com"},
                "port": null,
                "params": [["user", "phone"]],
                "headers": [["Subject", "x"], ["Flag", null]],
                "fragment": null,
            })
        );
    }

    #[test]
    fn deserialize_canonizes() {
        let uri: SipUri = serde_json::from_value(json!({
            "user": "a b;c",
            "host": {"hostname": "EXAMPLE%2ecom"},
            "params": [["x", "a@b"], ["", null]],
            "headers": [["h", null], ["n", "a b"]],
        }))
        .unwrap();
        assert_eq!(uri.to_string(), "a%20b%3Bc@example.com;x=a%40b?h&n=a%20b");

        let tel: TelUri = serde_json::from_value(json!({"number": "+1555;x=1"})).unwrap();
        assert_eq!(tel, TelUri::new("+1555;x=1"));

        let urn: UrnUri = serde_json::from_value(json!({"nid": "SERVICE", "nss": "sos"})).unwrap();
        assert_eq!(urn, UrnUri::new("service", "sos"));

        let host: Host = serde_json::from_value(json!({"hostname": "198.51.100.1"})).unwrap();
        assert_eq!(
            host,
            Host::Ipv4(
                "198.51.100.1"
                    .parse()
                    .unwrap()
            )
        );
    }

    fn full_sip() -> SipUri {
        SipUri::new(Host::Ipv6(
            "2001:db8::1"
                .parse()
                .unwrap(),
        ))
        .with_user("alice")
        .with_user_param("cpc", Some("emergency"))
        .with_password("pw")
        .with_port(5061)
        .with_param("lr", None)
        .with_header("Subject", Some("a b"))
        .with_header("Flag", None)
        .with_fragment("f")
    }

    fn ipv4_sip() -> SipUri {
        SipUri::new(Host::Ipv4(
            "198.51.100.1"
                .parse()
                .unwrap(),
        ))
        .with_user("1.2.3.4")
    }

    fn full_tel() -> TelUri {
        TelUri::new("+15551234567")
            .with_param("cpc", Some("emergency"))
            .with_fragment("x")
    }

    fn full_urn() -> UrnUri {
        UrnUri::new("service", "sos")
            .with_r_component("r")
            .with_q_component("q")
            .with_f_component("f")
    }

    #[test]
    fn every_variant_round_trips() {
        round_trip(full_sip().into());
        round_trip(ipv4_sip().into());
        round_trip(full_tel().into());
        round_trip(full_urn().into());
        round_trip(Uri::Other(
            OtherUri::new(Some("HTTPS"), "//example.com").unwrap(),
        ));
        round_trip(Uri::Other(OtherUri::new(None, "*").unwrap()));
    }

    #[test]
    fn uri_is_tagged_by_kind() {
        let other = Uri::Other(OtherUri::new(Some("https"), "//example.com").unwrap());
        assert_eq!(
            serde_json::to_value(&other).unwrap(),
            json!({"other": {"scheme": "https", "rest": "//example.com"}})
        );
        let host = Host::Ipv4(
            "198.51.100.1"
                .parse()
                .unwrap(),
        );
        assert_eq!(
            serde_json::to_value(host).unwrap(),
            json!({"ipv4": "198.51.100.1"})
        );
    }

    #[test]
    fn other_uri_rejects_what_new_rejects() {
        for value in [
            json!({"scheme": "sip", "rest": "alice@example.com"}),
            json!({"scheme": "<sip", "rest": "x"}),
            json!({}),
        ] {
            let err = serde_json::from_value::<OtherUri>(value).unwrap_err();
            assert!(!err
                .to_string()
                .contains("alice"));
        }
    }

    const MARK: &str = "MARK3R";
    const MARK_NUMBER: u64 = 271828182845904;

    fn pointers(value: &Value, at: String, out: &mut Vec<String>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    let key = key
                        .replace('~', "~0")
                        .replace('/', "~1");
                    pointers(child, format!("{at}/{key}"), out);
                }
            }
            Value::Array(items) => {
                for (i, child) in items
                    .iter()
                    .enumerate()
                {
                    pointers(child, format!("{at}/{i}"), out);
                }
            }
            _ => {}
        }
        out.push(at);
    }

    /// Replaces every node of `sample`'s serialized form with each marker in
    /// turn and checks no resulting error quotes it.
    fn errors_quote_no_marker<T: Serialize + DeserializeOwned>(sample: &T) {
        let value = serde_json::to_value(sample).unwrap();
        let mut at = Vec::new();
        pointers(&value, String::new(), &mut at);
        let markers = [
            json!(MARK),
            json!(MARK_NUMBER),
            json!(true),
            json!({MARK: 1}),
            json!([MARK]),
        ];
        let mut failures = 0;
        for pointer in &at {
            for marker in &markers {
                let mut doc = value.clone();
                *doc.pointer_mut(pointer)
                    .unwrap() = marker.clone();
                let text = doc.to_string();
                let errors = [
                    serde_json::from_value::<T>(doc).err(),
                    serde_json::from_str::<T>(&text).err(),
                ];
                for err in errors
                    .into_iter()
                    .flatten()
                {
                    failures += 1;
                    let message = err.to_string();
                    assert!(
                        !message.contains(MARK) && !message.contains(&MARK_NUMBER.to_string()),
                        "{} at {pointer:?} with {marker}: {message}",
                        std::any::type_name::<T>(),
                    );
                }
            }
        }
        assert!(failures > 0, "{}", std::any::type_name::<T>());
    }

    #[test]
    fn errors_never_quote_the_value() {
        let other = OtherUri::new(Some("https"), "//example.com").unwrap();
        let named = SipUri::new(Host::Hostname("example.com".into())).with_scheme(Scheme::Sips);
        for sip in [full_sip(), ipv4_sip(), named] {
            errors_quote_no_marker(&sip);
            errors_quote_no_marker(
                &sip.clone()
                    .into_parts(),
            );
            errors_quote_no_marker(&Uri::from(sip));
        }
        errors_quote_no_marker(&full_tel());
        errors_quote_no_marker(&full_tel().into_parts());
        errors_quote_no_marker(&Uri::from(full_tel()));
        errors_quote_no_marker(&full_urn());
        errors_quote_no_marker(&full_urn().into_parts());
        errors_quote_no_marker(&Uri::from(full_urn()));
        errors_quote_no_marker(&other);
        errors_quote_no_marker(&Uri::from(other));
        for host in [
            Host::Ipv4(
                "198.51.100.1"
                    .parse()
                    .unwrap(),
            ),
            Host::Ipv6(
                "2001:db8::1"
                    .parse()
                    .unwrap(),
            ),
            Host::Hostname("example.com".into()),
        ] {
            errors_quote_no_marker(&host);
        }
        errors_quote_no_marker(&Hostname::from("example.com"));
        errors_quote_no_marker(&Scheme::Sip);
        errors_quote_no_marker(&Params::from(vec![
            ("lr".to_owned(), None),
            ("transport".to_owned(), Some("tcp".to_owned())),
        ]));
        errors_quote_no_marker(&UserParams::from(vec![(
            "cpc".to_owned(),
            Some("x".to_owned()),
        )]));
        errors_quote_no_marker(&Headers::from(vec![(
            "Subject".to_owned(),
            Some("x".to_owned()),
        )]));
    }

    #[test]
    fn a_list_past_the_limit_fails_naming_its_field() {
        let pairs = json!([["a", "1"], ["MARKER", null]]);
        let err = crate::params::limit::with(7, || {
            serde_json::from_value::<Headers>(pairs.clone())
                .unwrap_err()
                .to_string()
        });
        assert!(err.contains("`headers`"), "{err}");
        assert!(err.contains("u32::MAX"), "{err}");
        assert!(!err.contains("MARKER"), "{err}");
        assert!(serde_json::from_value::<Headers>(pairs).is_ok());
    }

    #[test]
    fn error_names_its_field() {
        let message = |value: Value| {
            serde_json::from_value::<Uri>(value)
                .unwrap_err()
                .to_string()
        };
        assert!(message(json!({"sip": {"port": "x"}})).contains("`port`"));
        assert!(message(json!({"sip": {"host": {"ipv4": 1}}})).contains("`ipv4`"));
        assert!(message(json!({"tel": {"params": [[1, null]]}})).contains("`params` name"));
        assert!(message(json!({"urn": {"nid": []}})).contains("`nid`"));
    }

    #[test]
    fn parts_default_missing_fields() {
        assert_eq!(
            serde_json::from_value::<SipUriParts>(json!({})).unwrap(),
            SipUriParts::default()
        );
        assert_eq!(
            serde_json::from_value::<TelUriParts>(json!({"x": 1})).unwrap(),
            TelUriParts::default()
        );
        assert_eq!(
            serde_json::from_value::<UrnUriParts>(json!({})).unwrap(),
            UrnUriParts::default()
        );
    }
}
