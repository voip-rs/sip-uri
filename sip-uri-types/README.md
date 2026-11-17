# sip-uri-types

SIP/SIPS (RFC 3261), tel: (RFC 3966) and URN (RFC 8141) URI value types for Rust.

Each component is held in one canonical form, whether the value was built or parsed, so equality, hashing and `Display` do not depend on how the URI was spelled. Crates that expose a URI in their public API name these types, which change only when a value's identity does.

Parsing, warnings and log redaction live in [sip-uri](https://crates.io/crates/sip-uri), which parses into these types through its [`UriParse`](https://docs.rs/sip-uri/latest/sip_uri/trait.UriParse.html) trait and re-exports them. This crate has no parser of its own and no `FromStr`.

```sh
cargo add sip-uri-types
```

```rust
use sip_uri_types::{Host, SipUri, TelUri};

let uri = SipUri::new(Host::Hostname("example.com".into()))
    .with_user("+15551234567")
    .with_param("user", Some("phone"));
assert_eq!(uri.to_string(), "sip:+15551234567@example.com;user=phone");

// Builder input is canonized: a delimiter cannot add a param.
let tel = TelUri::new("+1555;x=1");
assert_eq!(tel.number(), Some("+1555%3Bx=1"));
```

## Types

| Type | Description |
|---|---|
| `Uri` | `Sip`, `Tel`, `Urn` or `Other` |
| `SipUri`, `TelUri`, `UrnUri` | URI values with builders and accessors |
| `SipUriParts`, `TelUriParts`, `UrnUriParts` | Public-field components; `From` canonizes them into the URI, `into_parts()` gives them back |
| `Params`, `UserParams`, `Headers` | Ordered `(name, value)` pairs, canonized on insertion; `iter()`, `retain()`, and case-insensitive `get()`, `set()` and `remove()`; `params_mut()` and `headers_mut()` edit them in place, and `SipUri::edit_user_params()` lends the user-params to a closure |
| `OtherUri` | Text with an unrecognized scheme, or none, scheme lowercased; `OtherUri::new(scheme, rest)`, refusing with an `OtherUriError` |
| `KindMismatchError` | Error of `TryFrom<Uri>` for `SipUri`, `TelUri`, `UrnUri` and `OtherUri`; names both kinds, `into_uri()` hands the `Uri` back |
| `Host`, `Hostname`, `Bare` | IPv4, IPv6 or a canonical hostname; `bare()` renders IPv6 without brackets |
| `UserHost`, `AssignedName` | `Display` adapters from `SipUri::user_host()` (`user@host:port`) and `UrnUri::assigned_name()` (`urn:NID:NSS`) |
| `Scheme` | `Sip` or `Sips` |
| `UriHeaderEquivalence`, `ordered_headers_equivalent` | How two `Headers` sets compare in sip-uri's RFC equivalence: the trait a SIP header parser implements with each field's rules, and the default rule, rows sharing a name in order with exact values (RFC 3261 §7.3.1) |
| `encoding` | One encoder and one decoder per component a builder takes as text, between logical bytes and the text a builder holds unchanged: `encode_user`, `decode_user`, `encode_header`, … |

## Canonical form

Each component keeps its conformant characters literal, decodes escapes of unreserved characters only, and holds every other byte as an uppercase `%XX`; tel: numbers, fragments and URN components decode none. An escaped reserved character stays escaped, so `%2B1` and `+1` are distinct users.

Builders and parts structs take URI text, not logical values: `with_user("%2B1")` holds `%2B1`. The `encoding` module converts between logical bytes and that text: `decode_user("%2B1")` gives the bytes `+1`, and `encode_user("a b")` gives `a%20b`, escaping only what the user part does not keep literal.

Hostnames are lowercased as ASCII only, with no IDNA and no non-ASCII case folding, and a URI holds a hostname that reads as an IPv4 address as `Host::Ipv4`. IPv6 renders as the standard library displays it, and an IPv6 zone ID is not representable.

## Equality

`Eq` and `Hash` compare the canonical form component by component: identity, never RFC equivalence. Param order, param and header name case, a tel: number's visual separators, a hostname's trailing dot and a URN's r-, q- and f-components all count.

## Serde

The optional `serde` feature serializes a value as its parts and deserializes through the same canonizing constructor, so a deserialized value equals the one `From` its parts would build. `Uri` and `Host` are tagged by kind (`{"sip": {…}}`, `{"hostname": "example.com"}`), `Scheme` is `"sip"` or `"sips"`, params and headers are sequences of `[name, value]` pairs with `null` for no value, missing fields take their defaults and unknown ones are ignored; a SIP URI read without `scheme` has none. To read and write a URI as text, use the adapters in sip-uri's `serde_str` module.

## Stability

The serde shape is under the same semver as the fields; a field added to a parts struct is a minor release. Data written by an older release reads back only in a self-describing format such as JSON, YAML or CBOR; a positional format such as bincode or postcard does not survive an added field. The URI kinds with their own type are fixed: a URI with any other scheme is `Uri::Other`. `Uri` and `Scheme` are therefore exhaustive, so a `match` on them needs no wildcard arm.

## Dependencies

None, beyond the optional `serde`.

Rust 1.70 or newer; the `serde` feature needs 1.71, which serde_derive's dependencies require. Raising either is a minor release.

## License

MIT OR Apache-2.0 — see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
