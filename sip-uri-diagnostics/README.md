# sip-uri-diagnostics

The warning and error vocabulary [sip-uri](https://crates.io/crates/sip-uri) reports SIP/SIPS (RFC 3261), tel: (RFC 3966) and URN (RFC 8141) URI parses in.

A crate that hands sip-uri's diagnostics to its own callers names these types in its public API. They live in their own 1.x crate so that sip-uri's 0.x releases do not break that API: this crate only grows, and a new warning code or component is a minor release. sip-uri re-exports every item unchanged, so code that only parses can keep `use sip_uri::...`.

```sh
cargo add sip-uri-diagnostics
```

```rust
use sip_uri_diagnostics::{Component, ParseWarning, WarningCode, WarningKind};

let w = ParseWarning::new(Component::Port, WarningCode::SignedPort, Some(16));
assert_eq!(w.kind(), WarningKind::Recovered);
assert_eq!(w.to_string(), "port: signed port at byte 16");
assert_eq!(w.code.as_str(), "signed-port");
```

## Types

| Type | Description |
|---|---|
| `ParseWarning` | A grammar breach the parser accepted: component, code and byte position; `kind()` is its code's |
| `Component` | The URI component a warning is in; `ALL` lists every component, `as_str()` gives a stable kebab-case name |
| `WarningCode` | What a warning found; `ALL` lists every code, `as_str()` gives a stable kebab-case name, `kind()` and `describe()` its kind and description |
| `WarningKind` | Whether the parsed value still carries what was sent (`Recovered`) or dropped it (`Lost`) |
| `ParseError` | `Empty`, `SchemeMismatch`, `NoHost`, or `NonConformant` from a strict parse |

No type here holds input text, and no `Display` quotes it: a warning names where the breach is, never what text it was, since the user part is often a phone number and warnings are what consumers log.

## Dependencies

None.

Rust 1.70 or newer. Raising it is a minor release.

## License

MIT OR Apache-2.0 — see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
