# Migrating from sip-uri 0.2 to 0.3

sip-uri 0.3 changes three things about the crate:

- Parsing rarely fails. A non-conformant URI parses, and each breach of the grammar comes back as a typed warning.
- Every URI component has one canonical form, whether the value was parsed or built.
- The value types moved to their own crate, sip-uri-types, which changes only when a value's identity does.

Most of the API changes on this page follow from one of those three.

## Crates: sip-uri-types and sip-uri

The URI types (`Uri`, `SipUri`, `TelUri`, `UrnUri`, `OtherUri`, `Host`, `Hostname`, `Scheme`) are now defined in [sip-uri-types](https://crates.io/crates/sip-uri-types). sip-uri parses into them and re-exports them, so `use sip_uri::SipUri` keeps working.

Which to depend on:

- An application that parses URIs depends on `sip-uri`.
- A library that only holds or passes URIs, and names them in its public API, depends on `sip-uri-types`.

sip-uri-types changes only when a value's identity changes: its components, its canonical form, equality, or Display. Crates that exchange URIs share only that small, slow-moving surface, so sip-uri's parse policy can change without breaking a library that exposes `sip_uri_types::Uri`. Before 0.3, every sip-uri minor was also a breaking release for every crate that exposed a URI.

## Parsing is the `UriParse` trait, not `FromStr`

```rust,ignore
// 0.2
let uri: Uri = s.parse()?;
let sip: SipUri = s.parse()?;

// 0.3
use sip_uri::UriParse;
let uri = Uri::parse(s)?;
let sip = SipUri::parse(s)?;
```

Rust's orphan rule lets `FromStr` be implemented only in the crate that defines the type, or in the crate that defines the trait. The types live in sip-uri-types and the parser lives in sip-uri, so the parser is offered as an extension trait. `UriParse` has `parse`, `parse_with_warnings` and `parse_strict`, and it is implemented for `Uri`, `SipUri`, `TelUri`, `UrnUri` and `Host`. It also works as a function value, for example `<Uri as UriParse>::parse` as a clap `value_parser`.

## Non-conformant input parses, with warnings

0.2.9 already had `parse_with_warnings`, `Parsed`, `ParseWarning`, `Component`, `WarningCode` and `WarningKind`, as inherent methods and types, for the few breaches it accepted; most grammar breaches were still an `Err`. In 0.3 most of those errors became warnings, with new `WarningCode`s, and the methods moved to the `UriParse` trait, which adds `parse_strict`. The parser returns whatever it could read and reports each breach:

```rust
use sip_uri::{Component, SipUri, UriParse, WarningCode};

let parsed = SipUri::parse_with_warnings("sip:+15551234567@example.com:+5060").unwrap();
assert_eq!(parsed.value.port(), Some(5060));
assert_eq!(parsed.warnings[0].component, Component::Port);
assert_eq!(parsed.warnings[0].code, WarningCode::SignedPort);
```

This matters wherever the application can't just drop the input. A server handling a call usually still has to handle it when the caller's URI is malformed, and what gets the sending side fixed is a report naming the breach. Rejecting loses the call; accepting silently loses the report.

A missing or unreadable component costs only that component. The accessors for components the grammar requires became `Option`:

| 0.2 | 0.3 |
|---|---|
| `SipUri::scheme() -> Scheme` | `Option<Scheme>` |
| `SipUri::host() -> &Host` | `Option<&Host>` |
| `TelUri::number() -> &str` | `Option<&str>` |
| `UrnUri::nid()`, `nss() -> &str` | `Option<&str>`, so write `urn.nid() == Some("service")` |
| `Uri::scheme() -> &str` | `Option<&str>`, lowercase for every variant |

A few inputs that were errors now parse to something:

- **Missing scheme:** `"joe@example.com"` as `SipUri` has scheme `None` and a `MissingScheme` warning. As `Uri`, it is `Uri::Other`, since `Uri` never guesses a type for scheme-less text. Parse it as `SipUri` when your context says it is SIP.
- **Wildcard:** `"*"` as `Uri` is `Uri::Other` with a `Wildcard` warning.
- **Brackets:** `"<sip:alice@example.com>"` as `Uri` stays `Uri::Other`, as in 0.2.9, now held escaped as `%3Csip:alice@example.com%3E` with an `InvalidScheme` warning; as `SipUri` it is no longer an error. That text is header grammar; see [Display names](#display-names-and-header-params) below.
- **Unreadable scheme:** as `SipUri`, text whose scheme cannot be read keeps everything up to `@` as the user part, and the `:` ending that prefix starts no password. `"<sip:+15551234567@example.com>"` has user `%3Csip%3A+15551234567` and an `InvalidScheme` warning, and the trailing `>` is dropped with a `TrailingContent` warning.

### Refusing non-conformant input

`parse` accepts exactly what `parse_with_warnings` accepts, and drops the warnings. So code that used a failed parse to refuse bad input has to ask explicitly:

```rust,ignore
// 0.2: a failed parse meant "malformed"
let Ok(uri) = s.parse::<SipUri>() else { return reject() };

// 0.3: parse_strict turns the first warning into the error
let Ok(uri) = SipUri::parse_strict(s) else { return reject() };

// 0.3: or look at the warnings and decide
let parsed = SipUri::parse_with_warnings(s)?;
if parsed.has_warnings() {
    log_discrepancy(&parsed.warnings);
}
```

`Parsed::into_strict()` does the same after the fact. The strict parser runs the lenient one and checks its warnings, so both accept and read input the same way; they differ only in what they return.

## One `ParseError` replaces the six error types

| 0.2 | 0.3 |
|---|---|
| `ParseSipUriError`, `ParseTelUriError`, `ParseUrnError`, `ParseHostError`, `ParseUriError`, `ParseNameAddrError` | `ParseError` |

At first sight this looks like a loss of detail. It isn't. Each 0.2 type was a newtype over a `String`: a message, often quoting the input, and nothing a program could match on. What set them apart was the type they came from, not anything about what went wrong.

In 0.3, nearly everything those messages described is a typed `ParseWarning`. Each warning says:

- which component broke (`Component`);
- what broke (`WarningCode`);
- the byte offset (`position`);
- whether the value survived (`WarningKind::Recovered`) or the component was dropped (`WarningKind::Lost`).

That leaves a lenient parse three errors — empty input, another type's scheme, and for `Host` alone no readable host — and a strict one a fourth:

```rust,ignore
#[non_exhaustive]
pub enum ParseError {
    /// The input is empty.
    Empty,
    /// The input's scheme names another kind of URI.
    SchemeMismatch,
    /// A standalone host's input holds no readable host.
    NoHost,
    /// A strict parse met a grammar breach.
    NonConformant(ParseWarning),
}
```

One type also means `?` works across `Uri`, `SipUri`, `TelUri`, `UrnUri` and `Host` with no `From` impls, and a caller can match on the cause, including the full warning under `parse_strict`.

Neither errors nor warnings quote the input. A user part often holds a phone number, and errors and warnings are what ends up in logs. The value itself is on the parsed struct for code entitled to read it.

## One canonical form per component

Every component holds its conformant characters literal, decodes escapes of unreserved characters only, and holds every other byte as an uppercase `%XX`; tel: numbers, fragments and URN components decode none. Builders, parts constructors, deserialization and the parser all go through the same canonizer. A user part also keeps a literal `#`, which phones send unescaped; `%23` stays a distinct value.

What changes for existing code:

| Input | 0.2 | 0.3 |
|---|---|---|
| user `a b` (a raw space) | kept raw | `a%20b`, with a warning |
| param `maddr=a@b` | kept raw | `a%40b`, with a warning |
| `%3B` in a user part | decoded to `;` | kept as `%3B`, since `;` there would start a user-param |
| `%2B` in a user part | decoded to `+` | kept as `%2B`: RFC 3261 counts an escaped reserved character distinct from the literal |
| `%61` in a user part | decoded to `a` | decoded to `a`, as every escape of an unreserved character is |
| param `x=a,b` | kept raw | `a%2Cb`, with a warning |
| `with_user("x;cpc=1@evil")` | written verbatim | escaped, so it can't add params or change the host |
| `TelUri::new("+1555;x=1")` | Display re-parses with a param `x` | `+1555%3Bx=1` |
| `Host::Hostname("EXAMPLE.COM".into())` | kept as given | `example.com`, equal to the parsed host |
| `Host::Hostname("198.51.100.1".into())` in a URI | kept as a hostname | held as `Host::IPv4`, equal to the parsed host |
| `Uri::Other` text with a space, CRLF or non-ASCII | kept raw | escaped as `%20`, `%0D%0A`, one `%XX` per byte; nothing is decoded |

Two consequences:

- Two spellings of one value compare equal, so `a b` and `a%20b` are the same URI.
- Display never prints a byte that changes how the URI parses, or that would break a header line. Caller-supplied text can't inject params, headers or a host.

Conformant input is unchanged, including phone numbers with `*`, `#` and `+`. `encoding::decode_user` returns the logical bytes of a user part when you need them, and the `encoding` module has an encoder and decoder like it for every component a builder takes.

Builders and parts structs take URI text, not logical values: `with_user("%2B1")` holds `%2B1`, distinct from `+1`. Code that fed a builder already-decoded text keeps working as long as that text holds no `%`.

## Constructing values

| 0.2 | 0.3 |
|---|---|
| builders only | builders, plus `SipUriParts`, `TelUriParts`, `UrnUriParts` through `From`, and `into_parts()` |
| `Uri::Other(String)` | `Uri::Other(OtherUri)`, built with `OtherUri::new(scheme, rest)`, which returns `Result<OtherUri, OtherUriError>` |
| `as_other() -> Option<&str>`, `into_other() -> Option<String>` | `Option<&OtherUri>`, `Option<OtherUri>`; `as_str()` or `to_string()` gives the text |
| `Host::Hostname(String)` | `Host::Hostname(Hostname)`; `"x".into()` still builds one, lowercased |

The parts structs are `#[non_exhaustive]`, so start from `Default` and assign fields:

```rust
use sip_uri::{Host, Scheme, SipUri, SipUriParts};

let mut parts = SipUriParts::default();
parts.scheme = Some(Scheme::Sip);
parts.user = Some("alice".into());
parts.host = Some(Host::Hostname("example.com".into()));
let uri = SipUri::from(parts);
assert_eq!(uri.to_string(), "sip:alice@example.com");
```

Every field starts absent, the scheme included: without `parts.scheme` the URI prints as `alice@example.com`.

A constructor given an empty component stores it as absent wherever its delimiter alone would re-parse as nothing: `with_user("")` gives a URI with no user, while `with_password("")` keeps an empty password.

## Params

`param()` on `SipUri` and `TelUri` returns `Option<Option<&str>>`, which tells a missing param from one present without a value:

```rust
use sip_uri::{SipUri, UriParse};

let uri = SipUri::parse("sip:example.com;transport=tcp;lr").unwrap();
assert_eq!(uri.param("transport"), Some(Some("tcp")));  // ;transport=tcp
assert_eq!(uri.param("lr"), Some(None));                // ;lr
assert_eq!(uri.param("maddr"), None);                   // absent
```

The new `user_param()` looks up the params inside the userinfo in the same way.

The collections are opaque types instead of slices of tuples, so their storage can change without breaking callers:

| 0.2 | 0.3 |
|---|---|
| `params() -> &[(String, Option<String>)]` | `&Params`; `iter()` yields `(&str, Option<&str>)`, `get()` looks up case-insensitively |
| `param(name) -> Option<&Option<String>>`, on `SipUri` and `TelUri` | `Option<Option<&str>>` |
| `user_params() -> &[(String, Option<String>)]` | `&UserParams`, the same API |
| `headers() -> &[(String, String)]` | `&Headers`, the same API; a value is `Option<&str>` |
| `header(name) -> Option<&str>` | `Option<Option<&str>>`, like `param()` |
| `with_header(name, value)` | `with_header(name, Some(value))`, the value an `Option<&str>` |
| `with_param(name, Some(value.into()))`, `with_user_param(…)` | `with_param(name, Some(value))`: values are `Option<&str>`, as in `Params::with` |
| `with_user_params(Vec<…>)` | takes a `UserParams`, or the same `Vec` through `From` |

`push`, `From` and `collect()` canonize each pair by its component's grammar, which is why user-params have a type of their own: `=` is literal in a user-param value and escaped in a URI param. A header written without `=`, as in `sip:example.com?Flag`, holds no value and prints as it came; 0.2 refused to parse it, and its builder could only write `?Flag=`.

## Logging

`Display` writes the user part and password. For logs, use the redacted rendering. By default it masks the whole userinfo (or a tel: number) and every URI header value, since headers such as `P-Asserted-Identity` carry identities; `HeaderMask::Visible` shows headers, `drop_headers()` leaves them out:

```rust
use sip_uri::{HeaderMask, Redaction, SipUri, UriParse, UriRedact, UserMask};

let uri = SipUri::parse("sip:+15551234567:pw@example.com?Subject=x").unwrap();
assert_eq!(uri.redacted(&Redaction::default()).to_string(), "sip:***@example.com?Subject=***");
let keep4 = Redaction::default().user(UserMask::KeepLast(4)).headers(HeaderMask::Visible);
assert_eq!(uri.redacted(&keep4).to_string(), "sip:+xxxxxxx4567:***@example.com?Subject=x");
```

`redacted()` comes from the `UriRedact` trait, so import it. It borrows the `Redaction`, which owns its policy (`params()` takes any iterator of names) and clones cheaply, so a policy read from configuration is built once and stored, for example in a logger struct.

`Debug` of `SipUri`, `SipUriParts` and `Uri` writes a password as `***`; 0.2's derived `Debug` printed it. The user part still prints as it is.

## Serde

A new `serde` feature serializes a value as its parts and deserializes it through the same canonizing constructor. To read and write a URI as text instead, use the adapters in `sip_uri::serde_str` with `#[serde(with = …)]`. The [README](../README.md#serde) shows both forms.

## Partial renderings are `Display` adapters

| 0.2 | 0.3 |
|---|---|
| `SipUri::user_host() -> String` | `UserHost<'_>`: user part, host and port, without user-params or password; `to_string()` gives the text |
| `UrnUri::assigned_name() -> String` | `AssignedName<'_>`: `urn:NID:NSS`; `to_string()` gives the text |

Each writes into a formatter without allocating, as `Host::bare()` does.

## Removed

| 0.2 | 0.3 |
|---|---|
| `NameAddr` | `sip_header::SipHeaderAddr`, or `Uri` for a bare URI |
| `Host::fmt_uri(f)` | `Display` |
| `sip_uri::decode_user` | `sip_uri::encoding::decode_user` |
| `sip_uri::encode_uri_header(&str) -> Cow<str>` | `sip_uri::encoding::encode_header(bytes) -> String` |

## Display names and header params

sip-uri parses URIs only (`addr-spec`). `"Alice" <sip:alice@example.com>;tag=abc` is SIP header grammar: parse it with [`SipHeaderAddr`](https://docs.rs/sip-header/%5E1.0.0-beta/sip_header/struct.SipHeaderAddr.html) from [sip-header](https://crates.io/crates/sip-header). Code written against 0.2's `NameAddr` moves there.

## Also new

- Conversions: `Host` from `Ipv4Addr`, `Ipv6Addr` and `IpAddr`, and `Host::ip()` back; `Uri` from `OtherUri`; `SipUri`, `TelUri` and `UrnUri` through `TryFrom<Uri>`, the `Uri` handed back as the error when it is another kind; `AsRef<str>` on `Hostname` and `OtherUri`; `Scheme::default_port()`; `Parsed::map()` to change the value and keep the warnings.
- A URI is edited in place: `SipUri::with_host()`, and `params_mut()`, `user_params_mut()` and `headers_mut()` on `SipUri` (`params_mut()` on `TelUri`) give the collection, whose `set()`, `remove()` and `retain()` work by case-insensitive name. `set()` replaces the first match in place, removes later ones and appends when absent; every insertion canonizes as the builders do.
- `Uri` and `Scheme` are exhaustive; 0.2 marked them `#[non_exhaustive]`. A `match` naming every variant needs no `_` arm, and one that has it gets an unreachable-pattern warning. The set is fixed: any other scheme is `Uri::Other`.
- `SipUri`, `TelUri`, `UrnUri` and `Uri` implement `Hash`, consistent with `Eq`. Both compare the canonical form component by component, never RFC 3261 §19.1.4 equivalence: param order, param and header name case, tel: visual separators and a hostname's trailing dot all count.
- `UriEquivalence::equivalent` compares URIs as RFC 3261 §19.1.4, RFC 3966 §4 and RFC 8141 §3 define equivalence: `sip:%61lice@example.com;transport=TCP` is equivalent to, though not `==`, `sip:alice@EXAMPLE.com;Transport=tcp`.
