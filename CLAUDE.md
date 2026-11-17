## Project Type

SIP/tel/URN URI library: RFC 3261 (SIP/SIPS), RFC 3966 (tel:), RFC 8141 (URN). Workspace of three crates: `sip-uri-types` holds the value types (the surface crates exchange URIs through, changing only on identity) and the header-comparison trait, `sip-uri-diagnostics` holds the warning and error vocabulary (it only grows), and `sip-uri` holds parsing, warning emission, equivalence and redaction and re-exports both. Decisions and their reasons live in the notes at the top of the file that enforces each one, and in `docs/design-rationale.md` when they span files.

The pre-commit hook runs across `--workspace --all-features`; it is the verification gate.

## No PII or Organization-Specific Data

**NEVER** include real phone numbers, real hostnames, organization names, internal URLs, or any other PII in source code, tests, or documentation. Use RFC-compliant test values only:

- Phone numbers: `+1555xxxxxxx` (555 prefix)
- IPv4: `198.51.100.x` or `203.0.113.x` (RFC 5737 TEST-NET)
- Domains: `example.com`, `example.org`, `example.net` (RFC 6761)
- IPv6: `2001:db8::x` (RFC 3849 documentation prefix)
- Organization names: "EXAMPLE CO", generic descriptions
- URN identifiers: synthetic hashes, `TEST` prefixes

The pre-commit hook runs gitleaks on staged content and refuses to commit without it.

## New RFC checks warn, never reject

A grammar check added to a parser pushes a `ParseWarning`; it never turns accepted input into an `Err`. The only errors are empty input, a scheme belonging to another type, a standalone host with nothing readable, and a SIP URI from which no component survives.

## Every component goes through its canonizer

Parser, builders, parts constructors and serde reach a component only through the data crate's canonizer; the parser never canonizes on its own.

## `#[non_exhaustive]` on every public enum and public-field struct

Single-field error newtypes are exempt, and so are `Uri` and `Scheme`, whose variant set the rationale fixes.

## No `assert!` / `unwrap()` in library code

## Redaction masks only what identifies a person

Before changing a default mask in `src/redact.rs`, adding a masked or public position, or reviewing either, read the notes at the top of `src/redact.rs`: public data stays visible, positions whose kind is unknown start masked, off-grammar identities are opt-in, and a default change is breaking.

## Scope Boundary

URIs only (`addr-spec`), never SIP header field grammar. A test value with percent-encoded header-level params (`;tag=`, `;serviceurn=`) is header grammar leaking in.

## No patch or pre-release versions in docs

Docs, rustdoc and READMEs name a major (`0.3`, `1.x`) at most, never `1.0.0-rc.3` or `0.3.2`. Only a migration guide names versions, and only major to major.

## Release Workflow

Use `/release` (`.claude/commands/release.md`); it owns the checks, changelog, tagging and publish steps.
