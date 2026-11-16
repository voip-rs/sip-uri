# Design Rationale

## Non-compliance warns; only nothing to read or another type's scheme is an error

Whatever an input breaks in the grammar, the parser returns what it could read and reports each breach as a typed warning beside the value. `Err` means only that there is nothing to read, which for a standalone host includes text holding no readable host and for a SIP URI text from which no component survives, or that a present scheme names a different kind of URI; `Uri` answers the latter with `Other`. A server handling a call often cannot refuse it over a malformed caller URI, yet the sender only gets fixed through a report naming the breach, so rejecting loses the call and silent acceptance loses the report.

`UriParse::parse_with_warnings` is the base; `parse` discards the warnings, and `parse_strict` turns the first one into the error. All three run one path, so a check exists once, as a warning code, and tightening one never narrows what the lenient parsers accept.

## URI values live in sip-uri-types; parsing lives in sip-uri

A value's identity, meaning its components, canonical form, equality and Display, belongs to sip-uri-types, which downstream crates name in their public APIs. Parsing, warnings, strict mode and redaction belong to sip-uri. Crates exchange URIs through these types, so the surface they share must move as little as possible: the types change only when a value's identity does, while parse policy changes freely without reaching them. The orphan rule leaves the data types without `FromStr`, so parsing is spelled through sip-uri's extension trait.

Params and headers are opaque collections, so their storage can change without reaching callers; the serde shape is under the same contract as the fields. The set of URI kinds with their own type is fixed, since a new one would turn stored `Other` values into another variant; `Uri` and `Scheme` are therefore exhaustive, and `Other` is the arm for every other scheme.

## A spec-required component holds absence, and a warning reports it

A scheme, host, NID, NSS or tel number that is missing or unreadable is `None` with a warning, rather than the whole URI failing. A broken port or host costs that field, never the user part beside it. A constructor given an empty component holds it as absent wherever its delimiter printed alone would re-parse as nothing. User-params exist only beside a user, so every path that changes either one, mutable access included, restores that pairing.

## Text outside the URI grammar is kept, never stripped

The parser never trims whitespace or removes brackets: text it cannot place stays, escaped, in the component it falls in, with a warning, and stripping what a header wrapped around the URI is the caller's job. Only text after the host or port that starts no component is dropped, and its warning says so. When the scheme cannot be read, everything up to `@` is the user part and its `:` starts no password, so a number behind a malformed prefix lands where a reader looks for it.

## A warning names the component and position, never the text

A warning carries which component broke, a typed code, the byte offset and whether the value survived, but no copy of the offending text. The user part is often a phone number and warnings are what consumers log; the value itself is on the parsed struct for anyone entitled to it. Error messages follow the same rule.

## `param-unreserved` accepts `@` and `,`, with a warning

Real SIP traffic and the sofia-sip torture corpus put both unescaped in URI params. They parse as param characters and are reported, since the grammar does not allow them. The value holds both escaped: a literal `@` in a URI without userinfo reads as the userinfo delimiter, and a literal `,` would make the Display of strictly conformant input fail a strict parse.

## Every component holds one canonical form; a user part keeps a literal `#`

Each component keeps its conformant characters literal, decodes escapes of unreserved characters only, and holds every other byte as an uppercase escape, whether the value was parsed or built. A tel: number, a fragment and a URN component decode no escape at all, since their grammars give an escape no meaning to normalize. An escaped reserved character stays escaped: RFC 3261 counts it distinct from the literal, and decoding it can change how the value parses. Two spellings of one value therefore compare equal, and no value prints a byte that changes the parse or breaks a header line. A URI holds a hostname that reads as an IPv4 address as one. A user part also keeps a literal `#`, which phones send unescaped and dialplans compare as it stands, while `%23` stays a distinct value. The warning reports the breach; the escape only fixes how the value holds it.

## One canonizing constructor per component; the parser only splits and warns

Builders, parts constructors and deserialization reach each component through the same canonizer, and so does the parser, which splits the input and warns but never canonizes. A value has one identity whichever path made it, and caller-supplied text cannot inject params, headers or a host.

## Grammar predicates are private to each crate, tied by a test

The parser's grammar classes decide warnings, and sip-uri-types' literal sets decide canonical form. Neither crate exports its predicates, so parse policy can widen or narrow what it reports without reaching the data types. The one coupling, that conformant text is never re-encoded, is asserted by a byte sweep through the public constructors.

sip-uri-types does export one encoder and one decoder per component, because a crate holding a logical value needs the URI text a builder takes, and the reverse. They work on bytes, not text: an encoder escapes every byte its component does not keep literal and cannot fail, and a decoder returns bytes, since an escape may stand for part of a UTF-8 sequence or for none. A builder holds an encoder's output unchanged, so encoding, building and decoding returns the logical bytes. Hostnames and namespace identifiers have no pair, since lowercasing makes their bytes unrecoverable.

## Display is the only wire form; other renderings are adapters

Display emits the canonical form and is the one serializer, so `parse(display(v)) == v` holds for every constructible value, while `display(parse(x)) == x` does not. The exceptions are a scheme-less `SipUri` or `Other` whose text begins like a scheme, which includes any scheme-less `SipUri` with a password, a scheme-less `SipUri` inside `Uri`, which `Uri` reads as `Other`, an `Other` whose text after the scheme reads as a port, a tel: fragment with no params before it, whose `#` reads as a phone digit, an empty or dotted-quad hostname built as a `Host` variant directly, and a `SipUri` built holding no component, which prints as nothing; each re-parses as another reading or none. Canonical components are ASCII. Any component a value holds must be emitted by Display. Another rendering, such as a host without IPv6 brackets or a URI redacted for logs, is a method returning its own Display adapter, never a second serializer.

Display's output is API that no tooling guards: a trait impl cannot be deprecated and semver checks do not read output. A change to what it emits therefore ships only in a breaking release, after the rendering it replaces is available as an adapter.

## Debug masks what a log must not carry

Debug masks the password, and every type's Debug that holds a URI masks the same way, since debug output reaches logs without anyone choosing it. Logs that should carry a URI use sip-uri's redacted rendering, whose rules live in the notes of `src/redact.rs`.

## Equality is canonical-structural identity, never RFC equivalence

`Eq` and `Hash` compare the canonical form component by component, so param order, param and header name case, and a tel: number's visual separators all count. RFC equivalence rests on defaults and per-param rules that could not be revisited without changing the identity of every stored value, so it is a separate function in sip-uri, and the canonical form never reorders, lowercases or strips to approximate it.

Where the RFCs leave the comparison open, equivalence errs toward not equivalent: header values and fragments compare exactly, a param named more than once compares by its first value, and `transport` joins the params that must match whenever either side has one, as RFC 3261's own non-equivalence example requires.

## Serde is structured and goes through the constructor

A URI serializes as its parts and deserializes through the canonizing constructor, so a deserialized value holds the same canonical form as a parsed one. A string form in sip-uri-types would need a second parser there, drifting from sip-uri's; the string adapter lives in sip-uri for callers that want one. A deserialization error names the field and what it expected, never the value it read, since serde's own messages quote it.

## sip-uri parses URIs, never header fields

Only `addr-spec` is in scope; display names and header-level parameters belong to a SIP header parser. Text still wrapped in `<>` parses as `Other` with a scheme warning, and a test value carrying percent-encoded header params signals header grammar leaking into this layer.
