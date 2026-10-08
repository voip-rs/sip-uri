# sip-uri

SIP/SIPS, tel:, and URN parser for Rust.

Implements RFC 3261 (SIP-URI, SIPS-URI), RFC 3966 (tel-URI), and
RFC 8141 (URN) with hand-written parsing and per-component
percent-encoding.

The value types live in [sip-uri-types](https://crates.io/crates/sip-uri-types), which this crate parses into and re-exports. Crates that exchange URIs name `sip_uri_types` in their public APIs; it changes only when a value's identity does, so the surface they share stays small while parse policy here moves freely.

```rust
use sip_uri::{SipUri, UriParse};

let uri = SipUri::parse("sip:alice@example.com;transport=tcp").unwrap();
assert_eq!(uri.user(), Some("alice"));
assert_eq!(uri.host().unwrap().to_string(), "example.com");
assert_eq!(uri.param("transport"), Some(Some("tcp")));
```

`UriParse` is the parser for every URI type, so import it beside the type; [TelUri](#teluri) and [UrnUri](#urnuri) parse the same way.

```toml
[dependencies]
sip-uri = "0.3.0-rc.1"
```

## Types

| Type | Description |
|---|---|
| `SipUri` | SIP or SIPS URI with user, host, port, params, headers, fragment |
| `TelUri` | tel: URI with number, params, fragment |
| `UrnUri` | URN with NID, NSS, and optional r/q/f components |
| `OtherUri` | Text with an unrecognized scheme, or none, kept with its scheme lowercased and bytes that would break a header line escaped |
| `SipUriParts`, `TelUriParts`, `UrnUriParts` | Public-field components; `From` canonizes them into the URI, `into_parts()` gives them back |
| `Params`, `UserParams`, `Headers` | Ordered `(name, value)` pairs, canonized on insertion; `iter()`, `retain()`, and case-insensitive `get()`, `set()` and `remove()`; `SipUri::params_mut()` and its siblings edit them in place |
| `Host` | IPv4, IPv6, or `Hostname` (lowercase by construction) |
| `Scheme` | `Sip` or `Sips` |
| `ParseError` | `Empty`, `SchemeMismatch`, `NoHost` for a standalone host, or `NonConformant` from a strict parse |
| `Parsed` / `ParseWarning` | Value plus the grammar breaches the parser accepted |
| `Redaction` | What `UriRedact::redacted()` masks when a URI is rendered for logs |
| `UriEquivalence` | RFC 3261, RFC 3966 and RFC 8141 URI equivalence, apart from `Eq` |

`Uri`, `SipUri`, `TelUri`, `UrnUri` and `Host` implement `UriParse`, and every URI and host type implements `Display`, `Debug`, `Clone`, `PartialEq`, `Eq` and `Hash`.
Parsing is a trait, not `FromStr`, so bring `sip_uri::UriParse` into scope.
Schemes and hosts are case-insensitive and stored lowercase; parameter and
header lookup is case-insensitive. `Eq` and `Hash` are canonical-structural
identity, never RFC 3261 §19.1.4 URI equivalence: parameter order, parameter
and header name case, tel: visual separators and a hostname's trailing dot all
count. RFC equivalence is `UriEquivalence::equivalent`, after RFC 3261 §19.1.4,
RFC 3966 §4 and RFC 8141 §3. `Display` emits the canonical form, so a value re-parses from its
`Display` as itself, while `display(parse(x))` need not equal `x`. The
exceptions re-parse as another reading or none:

- a scheme-less `SipUri`, or `Other`, whose text begins like a scheme
- a scheme-less `SipUri` with a password, whose `:` reads as the end of a scheme
- a scheme-less `SipUri` inside `Uri`, which `Uri` reads as `Other`
- an `Other` whose text after the scheme reads as a port
- a tel: fragment with no params before it, whose `#` reads as a phone digit
- a `Host::Hostname` on its own that is empty or reads as an IPv4 address

## SipUri

```rust
use sip_uri::{Scheme, SipUri, UriParse};

// Full SIP URI with user-params, password, port, params, headers
let uri = SipUri::parse(
    "sips:+15551234567;cpc=ordinary:secret@[2001:db8::1]:5061;user=phone?Subject=test",
)
.unwrap();

assert_eq!(uri.scheme(), Some(Scheme::Sips));
assert_eq!(uri.user(), Some("+15551234567"));
assert_eq!(uri.user_param("cpc"), Some(Some("ordinary")));
assert_eq!(uri.password(), Some("secret"));
assert_eq!(uri.port(), Some(5061));
assert_eq!(uri.param("user"), Some(Some("phone")));
assert_eq!(uri.header("Subject"), Some(Some("test")));
```

### User-params

SIP URIs with `user=phone` follow the telephone-subscriber production from
RFC 3966. Parameters within the userinfo (before `@`) are split from the
user part and exposed via `user_params()`:

```rust
use sip_uri::{SipUri, UriParse};

// A telephone number with its tel: params, carried inside a SIP URI
let uri = SipUri::parse("sip:+15551234567;cpc=ordinary;ext=100@198.51.100.1;user=phone").unwrap();

assert_eq!(uri.user(), Some("+15551234567"));
assert_eq!(uri.user_params().len(), 2);
```

### Builder

```rust
use sip_uri::{SipUri, Scheme, Host};
use std::net::Ipv4Addr;

let uri = SipUri::new(Host::IPv4(Ipv4Addr::new(198, 51, 100, 1)))
    .with_scheme(Scheme::Sips)
    .with_user("+15551234567")
    .with_port(5061)
    .with_param("transport", Some("tcp"));

assert_eq!(uri.to_string(), "sips:+15551234567@198.51.100.1:5061;transport=tcp");
```

## Host

`Host` parses on its own, for slots that hold an address but are not URIs
(URI parameter values, SDP connection lines, log text). `Display` brackets IPv6
for URI position; `bare()` never brackets.

```rust
use sip_uri::{Host, UriParse};

let host = Host::parse("[2001:db8::1]").unwrap();
assert_eq!(host.to_string(), "[2001:db8::1]");
assert_eq!(host.bare().to_string(), "2001:db8::1");

// Hostnames, bare IPv4 and bare IPv6 parse too; a trailing port is dropped
// with a warning.
assert!(Host::parse("example.com").is_ok());
assert!(Host::parse("2001:db8::1").is_ok());
assert!(Host::parse_strict("example.com:5060").is_err());
```

Brackets are the `IPv6reference` production, so `[198.51.100.1]` is rejected.

## TelUri

```rust
use sip_uri::{TelUri, UriParse};

let uri = TelUri::parse("tel:+15551234567;ext=100").unwrap();
assert_eq!(uri.number(), Some("+15551234567"));
assert!(uri.is_global());
assert_eq!(uri.param("ext"), Some(Some("100")));

// Local numbers (no + prefix) carry a phone-context
let local = TelUri::parse("tel:7042;phone-context=example.com").unwrap();
assert!(!local.is_global());
```

## UrnUri

URN parsing per RFC 8141. SIP carries URNs as service identifiers (RFC 5031),
3GPP IMS service types, and device identifiers such as GSMA IMEI.

```rust
use sip_uri::{UriParse, UrnUri};

// Service URN (RFC 5031)
let urn = UrnUri::parse("urn:service:counseling").unwrap();
assert_eq!(urn.nid(), Some("service"));
assert_eq!(urn.nss(), Some("counseling"));

// IETF document identifier
let urn = UrnUri::parse("urn:ietf:rfc:3261").unwrap();
assert_eq!(urn.nid(), Some("ietf"));
assert_eq!(urn.nss(), Some("rfc:3261"));

// 3GPP IMS service
let urn = UrnUri::parse("urn:urn-7:3gpp-service.ims.icsi.mmtel").unwrap();
assert_eq!(urn.nid(), Some("urn-7"));

// Optional RFC 8141 components (resolution, query, fragment)
let urn = UrnUri::parse("urn:example:resource?+resolve?=query#section").unwrap();
assert_eq!(urn.r_component(), Some("resolve"));
assert_eq!(urn.q_component(), Some("query"));
assert_eq!(urn.f_component(), Some("section"));
assert_eq!(urn.assigned_name().to_string(), "urn:example:resource");
```

NID is validated per RFC 8141 (2-32 chars, alphanum bookends) and stored
lowercase. No URN component decodes an escape; escape hex is uppercased for
canonical comparison.

## Display names and header parameters (`name-addr`)

This crate parses URIs only. `"Alice" <sip:alice@example.com>;tag=abc` is
header grammar: parse it with
[`SipHeaderAddr`](https://docs.rs/sip-header/%5E1.0.0-beta/sip_header/struct.SipHeaderAddr.html)
from [`sip-header`](https://crates.io/crates/sip-header), which handles display
names, URIs and header-level parameters and re-exports this crate. Code
written against sip-uri 0.2's `NameAddr` moves to `SipHeaderAddr`.

## Percent-encoding

Every component holds one canonical form, whether parsed or built:

- Each URI component has its own allowed character set
- Escapes of unreserved characters are decoded (`%41` -> `A`); an escaped
  reserved character stays escaped, so `%2B` is not `+`; URN components, tel:
  numbers and fragments decode none
- Every other byte is an uppercase escape, whether it arrived escaped
  (`%3d` -> `%3D`) or literal (a space in a user part -> `%20`)
- A user part also keeps a literal `#`, and `%23` stays a distinct value
- Hostnames are lowercased as ASCII only, with no IDNA, and a URI holds a
  hostname that reads as an IPv4 address as `Host::IPv4`
- Builders and parts structs take URI text, so `with_user("%2B1")` holds
  `%2B1`, not `+1`
- `sip_uri::encoding` has one encoder and one decoder per component a builder
  takes as text, for callers holding the logical value rather than the
  canonical form:
  `encode_header` turns bytes into text `with_header` holds unchanged, and
  `decode_user` fully decodes a user part (every `%XX`, bytes out), e.g.
  FreeSWITCH's `sip_req_user`

```rust
use sip_uri::encoding::{decode_user, encode_user};
use sip_uri::{SipUri, UriParse};

// Percent-encoded quotes in user-part are preserved
let uri = SipUri::parse(r#"sip:%22foo%22@example.com"#).unwrap();
assert_eq!(uri.user(), Some(r#"%22foo%22"#));

// Full decode of a user part, and back
assert_eq!(decode_user("%2B15551234567").as_ref(), b"+15551234567");
assert_eq!(encode_user(r#""foo""#), "%22foo%22");
```

## Warnings

Parsing is best-effort: whatever an input breaks in the grammar, the parser
returns what it could read, and `parse_with_warnings` reports each breach as a
typed `ParseWarning` (component, code, byte position, whether the value
survived). A missing or unreadable scheme, host, port, number, NID or NSS is
`None` with a warning. The only errors are empty input, a scheme that belongs
to another type (`tel:` parsed as `SipUri`), and, for `Host` alone, input with
no readable host; `Uri` keeps anything else as `Other`. `parse` accepts exactly
the same input and drops the warnings.
Warnings never quote the input, since a user part often holds a phone number.

```rust
use sip_uri::{Component, SipUri, UriParse, WarningCode};

let parsed = SipUri::parse_with_warnings("sip:+15551234567@example.com:+5060").unwrap();
assert_eq!(parsed.value.port(), Some(5060));
assert_eq!(parsed.warnings[0].component, Component::Port);
assert_eq!(parsed.warnings[0].code, WarningCode::SignedPort);
```

`parse_strict` runs the same parser and returns the first warning as
`ParseError::NonConformant`, for callers that must refuse non-conformant
input.

## Logging

`Display` writes the user part and password. For logs, `UriRedact::redacted()` renders through a `Redaction`, and the caller relaxes it per deployment policy. By default:

- the whole userinfo, or a tel: number, becomes `***`;
- every URI header value becomes `***` (`?Subject=***`), since headers such as `P-Asserted-Identity` carry identities; a header without a value stays its name alone;
- params are shown, except those named in `params()`.

A `Redaction` owns its policy and clones cheaply, so one built from configuration is stored once and lent to each `redacted(&how)` call. `Debug` writes a password as `***` but the user part as it is.

```rust
use sip_uri::{HeaderMask, Redaction, SipUri, UriParse, UriRedact, UserMask};

let uri = SipUri::parse("sip:+15551234567:pw@example.com?Subject=x").unwrap();
assert_eq!(uri.redacted(&Redaction::default()).to_string(), "sip:***@example.com?Subject=***");
let keep4 = Redaction::default().user(UserMask::KeepLast(4)).headers(HeaderMask::Visible);
assert_eq!(uri.redacted(&keep4).to_string(), "sip:+xxxxxxx4567:***@example.com?Subject=x");
```

## Serde

The `serde` feature (Rust 1.71 or newer; the crates otherwise need 1.70, and raising either is a minor release) enables two forms. By default a value serializes as its parts and deserializes through the same canonizing constructor as `From` a parts struct; that impl lives in sip-uri-types. `Uri` and `Host` are tagged by kind:

```json
{"sip": {"scheme": "sip", "user": "alice", "user_params": [], "password": null,
         "host": {"hostname": "example.com"}, "port": null,
         "params": [["transport", "tcp"]], "headers": [["Subject", "x"], ["Flag", null]],
         "fragment": null}}
```

Params, user-params and headers are sequences of `[name, value]` pairs, the value null when absent.

A field that carries the URI as text uses an adapter from `sip_uri::serde_str`, which writes `Display` and reads with the lenient parser:

```rust
use serde::{Deserialize, Serialize};
use sip_uri::{SipUri, Uri};

#[derive(Serialize, Deserialize)]
struct Call {
    #[serde(with = "sip_uri::serde_str::uri")]
    to: Uri,
    #[serde(with = "sip_uri::serde_str::sip_uri::option", default)]
    contact: Option<SipUri>,
}
```

Adapters exist for `uri`, `sip_uri`, `tel_uri`, `urn_uri` and `host`, each with an `option` submodule. A read that fails reports the `ParseError`, or the type it expected, never the value it read.

## Migrating from 0.2

0.3 parses non-conformant input with typed warnings instead of failing, holds every component in one canonical form, and moves the value types to sip-uri-types. [docs/migrating-from-0.2.md](docs/migrating-from-0.2.md) explains each change and why, with before/after code.

## Design

- **Minimal dependencies** — sip-uri depends on sip-uri-types and, with the `serde` feature, serde; sip-uri-types depends on nothing but an optional `serde`. Not even `percent-encoding`: the subset needed is trivial and avoids transitive dep churn.
- **Hand-written parser** — the SIP URI grammar is regular enough that nom/regex
  are unnecessary overhead. Parsing follows the sofia-sip two-phase `@` discovery
  algorithm for correct handling of reserved characters in user-parts.
- **Case-insensitive where required** — scheme and parameter name lookup are
  case-insensitive per RFC. Host names are lowercased.
- **`#[non_exhaustive]`** — on every public enum and public-field struct but `Uri` and `Scheme`, whose variant set is fixed, so a `match` on them needs no wildcard arm.
- **Fragment support** — `SipUri` and `TelUri` keep a `#fragment`, which
  neither RFC defines, with an `UnexpectedFragment` warning, matching sofia-sip.
- **Any-scheme fallback** — `Uri::Other` keeps URIs with unrecognized schemes
  (http, https, data, etc.) as sent apart from a lowercased scheme and escaped
  bytes that would break a header line, decoding nothing, so header values
  like `Call-Info` that carry non-SIP URIs still parse.

## RFC coverage

- **RFC 3261 19/25** — SIP-URI, SIPS-URI syntax, percent-encoding
- **RFC 3966** — tel-URI (global/local numbers, visual separators, parameters)
- **RFC 8141** — URN syntax (NID, NSS, r/q/f components)

## Examples

`fs-sip-uri` lets a FreeSWITCH XML dialplan parse a URI properly instead of
applying regexes to a header value. See
[examples/freeswitch/README.md](examples/freeswitch/README.md) for the dialplan
syntax, field list and error contract. Build it statically so it runs under
whatever userspace FreeSWITCH itself runs under:

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --profile release-min --target x86_64-unknown-linux-musl --example fs-sip-uri
```

## Development

`hooks/install.sh` symlinks two hooks into `.git/hooks`, removing any local
core.hooksPath that would bypass them. pre-commit refuses Cargo.lock staged on
a branch, then runs fmt, clippy, doc coverage, tests and gitleaks on the staged
content; pre-push refuses a branch whose tip tracks Cargo.lock.

## Other Rust SIP URI crates

- [rsip](https://crates.io/crates/rsip) — full SIP library with heavy deps
  (nom, bytes, md5, sha2, uuid). No tel: URI support, no user-param extraction.
- [rvoip-sip-core](https://crates.io/crates/rvoip-sip-core) — alpha with a
  massive dependency tree.

Neither is a focused URI-only parser without third-party dependencies.

## License

MIT OR Apache-2.0 — see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
