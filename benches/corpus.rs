//! Synthetic URIs shaped like NG9-1-1 SIP traffic, for the parse benchmark.
//!
//! The values follow the repository's test-data rules (555 numbers,
//! example.com, documentation addresses, synthetic URN identifiers). The
//! file depends on nothing in sip-uri, so a comparison harness for another
//! release can include it with `#[path]` and measure the same inputs.

/// Request-URI, From, To, Contact and Route shapes.
pub const SIP: &[&str] = &[
    "sip:+15551234567@example.com",
    "sip:+15551234567;cpc=emergency;npdi@example.com;user=phone",
    "sip:alice@example.com:5060;transport=tcp",
    "sips:bob@psap.example.org;transport=tls;lr",
    "sip:911@198.51.100.10:5060",
    "sip:+15559876543@[2001:db8::10]:5061;transport=tls",
    "sip:example.net;lr;transport=tcp",
    "sip:caller@203.0.113.7;ob;rinstance=0123456789abcdef",
    "sip:%2B15551234567@example.com;user=phone",
    "sip:alice:secret@example.com?Subject=lunch&Priority=urgent",
    "sip:bcf.example.com;transport=tcp;lr;r2=on;ftag=a1b2c3",
    "sip:+15551112222;phone-context=example.com@example.com",
];

/// `tel:` shapes.
pub const TEL: &[&str] = &[
    "tel:+15551234567",
    "tel:+1-555-123-4567;ext=1234",
    "tel:5551234;phone-context=+1555",
    "tel:5551234;phone-context=example.com;isub=12",
    "tel:+15551234567;npdi;rn=+15559876543",
];

/// URN shapes.
pub const URN: &[&str] = &[
    "urn:service:sos",
    "urn:service:sos.police",
    "urn:nena:service:responder.police",
    "urn:emergency:uid:callid:TEST0123456789:bcf.example.com",
    "urn:emergency:uid:incidentid:TEST9876543210:psap.example.com",
    "urn:uuid:00000000-0000-4000-8000-000000000000",
];

/// Text `Uri` keeps as `Other`.
pub const OTHER: &[&str] = &[
    "https://adr.example.com/AdditionalData/TEST01",
    "cid:part1@example.com",
    "<sip:+15551234567@example.com>",
    "+15551234567@example.com",
];

/// SIP URIs whose params and headers the lookup benchmark reads, with the
/// names it looks up, present and absent.
pub const LOOKUP_URI: &str =
    "sip:+15551234567;cpc=emergency@example.com;user=phone;transport=tcp;lr;ftag=a1b2c3?Priority=urgent";
pub const LOOKUP_NAMES: &[&str] = &["transport", "lr", "user", "maddr", "Transport"];
