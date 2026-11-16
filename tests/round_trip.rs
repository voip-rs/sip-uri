use sip_uri::{
    Host, Hostname, OtherUri, OtherUriError, Params, Scheme, SipUri, SipUriParts, TelUri,
    TelUriParts, Uri, UriParse, UrnUri, UrnUriParts, UserParams, WarningCode,
};
use std::net::{Ipv4Addr, Ipv6Addr};

fn pairs<'a>(
    list: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
) -> Vec<(&'a str, Option<&'a str>)> {
    list.into_iter()
        .collect()
}

// ========================================================================
// Sofia-sip torture test cases (from torture_url.c)
// ========================================================================

#[test]
fn sofia_basic_sip() {
    let uri = SipUri::parse("sip:joe@example.com").unwrap();
    assert_eq!(uri.scheme(), Some(Scheme::Sip));
    assert_eq!(uri.user(), Some("joe"));
    assert_eq!(uri.host(), Some(&Host::Hostname("example.com".into())));
    assert_eq!(uri.port(), None);
    assert_eq!(uri.to_string(), "sip:joe@example.com");
}

#[test]
fn sofia_minimal() {
    let uri = SipUri::parse("sip:u@h").unwrap();
    assert_eq!(uri.user(), Some("u"));
    assert_eq!(uri.host(), Some(&Host::Hostname("h".into())));
    assert_eq!(uri.to_string(), "sip:u@h");
}

#[test]
fn sofia_host_only() {
    let uri = SipUri::parse("sip:test.host").unwrap();
    assert_eq!(uri.user(), None);
    assert_eq!(uri.host(), Some(&Host::Hostname("test.host".into())));
    assert_eq!(uri.to_string(), "sip:test.host");
}

#[test]
fn sofia_ipv4() {
    let uri = SipUri::parse("sip:198.51.100.55").unwrap();
    assert_eq!(
        uri.host(),
        Some(&Host::Ipv4(Ipv4Addr::new(198, 51, 100, 55)))
    );
}

#[test]
fn sofia_ipv4_with_port() {
    let uri = SipUri::parse("sip:198.51.100.55:5060").unwrap();
    assert_eq!(
        uri.host(),
        Some(&Host::Ipv4(Ipv4Addr::new(198, 51, 100, 55)))
    );
    assert_eq!(uri.port(), Some(5060));
}

#[test]
fn sofia_full_sips() {
    let uri = SipUri::parse("sips:user:pass@host:32;param=1?From=foo@bar&To=bar@baz").unwrap();
    assert_eq!(uri.scheme(), Some(Scheme::Sips));
    assert_eq!(uri.user(), Some("user"));
    assert_eq!(uri.password(), Some("pass"));
    assert_eq!(uri.host(), Some(&Host::Hostname("host".into())));
    assert_eq!(uri.port(), Some(32));
    assert_eq!(pairs(uri.params()), [("param", Some("1"))]);
    assert_eq!(uri.header("From"), Some(Some("foo%40bar")));
    assert_eq!(uri.header("To"), Some(Some("bar%40baz")));
}

#[test]
fn sofia_case_insensitive_scheme() {
    let uri = SipUri::parse("SIP:test@127.0.0.1:55").unwrap();
    assert_eq!(uri.scheme(), Some(Scheme::Sip));
    assert_eq!(uri.user(), Some("test"));
    assert_eq!(uri.port(), Some(55));
}

#[test]
fn sofia_empty_port() {
    // Empty port is valid per sofia-sip
    let uri = SipUri::parse("SIP:test@127.0.0.1:").unwrap();
    assert_eq!(uri.scheme(), Some(Scheme::Sip));
    assert_eq!(uri.port(), None);
}

#[test]
fn sofia_percent_encoded_quotes_in_user() {
    let uri = SipUri::parse("sip:%22foo%22@198.51.100.55:5060").unwrap();
    // %22 is double-quote, not unreserved, stays encoded
    assert_eq!(uri.user(), Some("%22foo%22"));
    assert_eq!(
        uri.host(),
        Some(&Host::Ipv4(Ipv4Addr::new(198, 51, 100, 55)))
    );
}

#[test]
fn sofia_user_with_slash_semicolon_password() {
    let uri = SipUri::parse("sip:user/path;tel-param:pass@host:32;param=1%3d%3d1").unwrap();
    assert_eq!(uri.user(), Some("user/path"));
    assert_eq!(pairs(uri.user_params()), [("tel-param", None)]);
    assert_eq!(uri.password(), Some("pass"));
    assert_eq!(uri.host(), Some(&Host::Hostname("host".into())));
    assert_eq!(uri.port(), Some(32));
    // %3d normalized to uppercase %3D
    assert_eq!(pairs(uri.params()), [("param", Some("1%3D%3D1"))]);
}

#[test]
fn sofia_reserved_chars_in_user_ipv6() {
    let uri = SipUri::parse("sip:&=+$,;?/:&=+$,@[::1]:56001;param=+$,/:@&").unwrap();
    assert_eq!(uri.user(), Some("&=+$,"));
    assert_eq!(uri.host(), Some(&Host::Ipv6(Ipv6Addr::LOCALHOST)));
    assert_eq!(uri.port(), Some(56001));
}

#[test]
fn sofia_hash_in_user() {
    // Sofia-sip compatibility: phones put unescaped # in user
    let uri = SipUri::parse("SIP:#**00**#;foo=/bar@127.0.0.1").unwrap();
    assert_eq!(uri.user(), Some("#**00**#"));
    assert_eq!(pairs(uri.user_params()), [("foo", Some("/bar"))]);
    assert_eq!(uri.host(), Some(&Host::Ipv4(Ipv4Addr::new(127, 0, 0, 1))));
}

#[test]
fn sofia_transport_and_maddr_params() {
    let uri = SipUri::parse("sip:u:p@host:5060;maddr=127.0.0.1;transport=tcp").unwrap();
    assert_eq!(uri.param("transport"), Some(Some("tcp")));
    assert_eq!(uri.param("maddr"), Some(Some("127.0.0.1")));
}

#[test]
fn sofia_param_without_value() {
    let uri = SipUri::parse("sip:u:p@host:5060;user=phone;ttl=1;isfocus").unwrap();
    assert_eq!(uri.param("user"), Some(Some("phone")));
    assert_eq!(uri.param("ttl"), Some(Some("1")));
    assert_eq!(uri.param("isfocus"), Some(None));
}

// ========================================================================
// Errors
// ========================================================================

#[test]
fn only_empty_input_and_foreign_schemes_fail() {
    assert!(SipUri::parse("").is_err());
    assert!(SipUri::parse("http://example.com").is_err());
    assert!(SipUri::parse("tel:+15551234567").is_err());
    assert!(TelUri::parse("sip:alice@example.com").is_err());
    assert!(UrnUri::parse("http:service:sos").is_err());
    assert!(Uri::parse("").is_err());
}

// ========================================================================
// Non-conformant input parses and warns
// ========================================================================

#[test]
fn nonconformant_sip_uris_parse_with_warnings() {
    for (input, code) in [
        ("sip:test@127.0.0.1::55", WarningCode::InvalidPort),
        ("sip:test@127.0.0.1:55:", WarningCode::InvalidPort),
        ("sip:test@127.0.0.1:sip", WarningCode::InvalidPort),
        ("joe@example.com", WarningCode::MissingScheme),
        (
            "4155551234@pbx.example.com;cpc=emergency",
            WarningCode::MissingScheme,
        ),
        ("sip:1411@", WarningCode::MissingHost),
        ("SIP:#**00**#;foo=/bar@#127.0.0.1", WarningCode::InvalidChar),
        ("SIP:#**00**#;foo=/bar;127.0.0.1", WarningCode::MissingHost),
    ] {
        let parsed = SipUri::parse_with_warnings(input).unwrap();
        assert!(
            parsed
                .warnings
                .iter()
                .any(|w| w.code == code),
            "{input}: expected {code:?} in {:?}",
            parsed.warnings
        );
        let reparsed = SipUri::parse(
            &parsed
                .value
                .to_string(),
        )
        .unwrap();
        assert_eq!(reparsed, parsed.value, "{input}");
    }
}

#[test]
fn wildcard_is_other_with_a_warning() {
    let parsed = Uri::parse_with_warnings("*").unwrap();
    assert_eq!(
        parsed
            .value
            .as_other()
            .map(|o| o.as_str()),
        Some("*")
    );
    assert_eq!(parsed.warnings[0].code, WarningCode::Wildcard);
}

// ========================================================================
// tel: URI test cases
// ========================================================================

#[test]
fn sofia_tel_basic() {
    let uri = TelUri::parse("tel:+12345678").unwrap();
    assert_eq!(uri.number(), Some("+12345678"));
    assert!(uri.is_global());
    assert!(uri
        .params()
        .is_empty());
    assert_eq!(uri.to_string(), "tel:+12345678");
}

#[test]
fn sofia_tel_with_params() {
    let uri = TelUri::parse("tel:+12345678;param=1;param=2").unwrap();
    assert_eq!(uri.number(), Some("+12345678"));
    assert_eq!(
        uri.params()
            .len(),
        2
    );
}

// ========================================================================
// NG911 patterns (from production, sanitized)
// ========================================================================

#[test]
fn ng911_user_params_ipv4_user_phone() {
    let sip =
        SipUri::parse("sip:+15551234567;cpc=emergency;oli=0@198.51.100.1;user=phone").unwrap();
    assert_eq!(sip.user(), Some("+15551234567"));
    assert_eq!(
        pairs(sip.user_params()),
        [("cpc", Some("emergency")), ("oli", Some("0"))]
    );
    assert_eq!(
        sip.host(),
        Some(&Host::Ipv4(Ipv4Addr::new(198, 51, 100, 1)))
    );
    assert_eq!(sip.param("user"), Some(Some("phone")));
}

#[test]
fn ng911_participantid() {
    let uri = SipUri::parse("sip:+15551234567@sip.example.com;participantid=abc123").unwrap();
    assert_eq!(uri.user(), Some("+15551234567"));
    assert_eq!(uri.param("participantid"), Some(Some("abc123")));
}

#[test]
fn ng911_participantid_no_user_part() {
    let uri = SipUri::parse("sip:sip.bcf.qc.core.ng.example.com;participantid=2").unwrap();
    assert_eq!(uri.user(), None);
    assert_eq!(
        uri.host()
            .unwrap(),
        &Host::Hostname("sip.bcf.qc.core.ng.example.com".into())
    );
    assert_eq!(uri.param("participantid"), Some(Some("2")));
    assert_eq!(
        uri.to_string(),
        "sip:sip.bcf.qc.core.ng.example.com;participantid=2"
    );
}

#[test]
fn ng911_participantid_no_user_part_extra_params() {
    let uri =
        SipUri::parse("sip:sip.bcf.qc.core.ng.example.com;participantid=2;user=phone").unwrap();
    assert_eq!(uri.user(), None);
    assert_eq!(uri.param("participantid"), Some(Some("2")));
    assert_eq!(uri.param("user"), Some(Some("phone")));
}

#[test]
fn ng911_participantid_non_word_value() {
    let uri =
        SipUri::parse("sip:sip.bcf.qc.core.ng.example.com;participantid=9f8e7d6c-1234").unwrap();
    assert_eq!(uri.param("participantid"), Some(Some("9f8e7d6c-1234")));
}

// Appending `@host` to a host-only URI yields a valid URI whose user part is a
// domain name and whose participantid became a user-param: nothing rejects it
// locally, the 404 only shows up on the wire.
#[test]
fn ng911_participantid_host_reused_as_user() {
    let uri = SipUri::parse(
        "sip:sip.bcf.qc.core.ng.example.com;participantid=2@sip.bcf.qc.core.ng.example.com",
    )
    .unwrap();
    assert_eq!(uri.user(), Some("sip.bcf.qc.core.ng.example.com"));
    assert_eq!(pairs(uri.user_params()), [("participantid", Some("2"))]);
    assert!(uri
        .params()
        .is_empty());
    assert_eq!(uri.param("participantid"), None);
}

#[test]
fn ng911_user_phone() {
    let sip = SipUri::parse("sip:1305@pbx.example.com;user=phone").unwrap();
    assert_eq!(sip.user(), Some("1305"));
    assert_eq!(sip.param("user"), Some(Some("phone")));
}

#[test]
fn ng911_ipv6_with_port() {
    let uri = SipUri::parse("sip:1411@[2001:db8::1]:5061;user=phone").unwrap();
    assert_eq!(uri.user(), Some("1411"));
    assert_eq!(
        uri.host()
            .unwrap(),
        &Host::Ipv6(
            "2001:db8::1"
                .parse::<Ipv6Addr>()
                .unwrap()
        )
    );
    assert_eq!(uri.port(), Some(5061));
    assert_eq!(uri.param("user"), Some(Some("phone")));
}

#[test]
fn ng911_ipv6_with_password() {
    let uri = SipUri::parse("sip:1411:secret@[2001:db8::1]:5061;user=phone").unwrap();
    assert_eq!(uri.user(), Some("1411"));
    assert_eq!(uri.password(), Some("secret"));
    assert_eq!(
        uri.host()
            .unwrap(),
        &Host::Ipv6(
            "2001:db8::1"
                .parse::<Ipv6Addr>()
                .unwrap()
        )
    );
    assert_eq!(uri.port(), Some(5061));
}

#[test]
fn ng911_tel_global() {
    let uri = TelUri::parse("tel:+15551234567").unwrap();
    assert_eq!(uri.number(), Some("+15551234567"));
    assert!(uri.is_global());
}

#[test]
fn ng911_session_id() {
    let sip = SipUri::parse("sip:session-id@focus.example.com").unwrap();
    assert_eq!(sip.user(), Some("session-id"));
    assert_eq!(
        sip.host(),
        Some(&Host::Hostname("focus.example.com".into()))
    );
}

#[test]
fn ng911_ipv6_without_port() {
    let sip = SipUri::parse("sip:+15551234567@[2001:db8::8];user=phone").unwrap();
    assert_eq!(sip.user(), Some("+15551234567"));
    assert_eq!(
        sip.host()
            .unwrap(),
        &Host::Ipv6(
            "2001:db8::8"
                .parse::<Ipv6Addr>()
                .unwrap()
        )
    );
}

#[test]
fn ng911_tel_with_cpc_emergency() {
    let tel = TelUri::parse("tel:+15551234567;cpc=emergency").unwrap();
    assert_eq!(tel.number(), Some("+15551234567"));
    assert_eq!(tel.param("cpc"), Some(Some("emergency")));
}

#[test]
fn ng911_tel_param_without_value() {
    let tel = TelUri::parse("tel:+15551234567;cpc=emergency;oli").unwrap();
    assert_eq!(tel.param("cpc"), Some(Some("emergency")));
    assert_eq!(tel.param("oli"), Some(None));
}

// ========================================================================
// Round-trip property: parse(display(parse(x))) == parse(x)
// ========================================================================

fn roundtrip_sip(input: &str) {
    let uri1 = SipUri::parse(input).unwrap();
    let displayed = uri1.to_string();
    let uri2 = SipUri::parse(&displayed).unwrap();
    assert_eq!(
        uri1, uri2,
        "roundtrip failed for '{input}' -> '{displayed}'"
    );
}

fn roundtrip_tel(input: &str) {
    let uri1 = TelUri::parse(input).unwrap();
    let displayed = uri1.to_string();
    let uri2 = TelUri::parse(&displayed).unwrap();
    assert_eq!(
        uri1, uri2,
        "roundtrip failed for '{input}' -> '{displayed}'"
    );
}

fn roundtrip_uri(input: &str) {
    let uri1 = Uri::parse(input).unwrap();
    let displayed = uri1.to_string();
    let uri2 = Uri::parse(&displayed).unwrap();
    assert_eq!(
        uri1, uri2,
        "roundtrip failed for '{input}' -> '{displayed}'"
    );
}

#[test]
fn roundtrip_sip_basic() {
    roundtrip_sip("sip:joe@example.com");
}

#[test]
fn roundtrip_sip_full() {
    roundtrip_sip("sips:user:pass@host:32;param=1?From=foo@bar&To=bar@baz");
}

#[test]
fn roundtrip_sip_ipv6() {
    roundtrip_sip("sip:user@[::1]:56001;transport=tcp");
}

#[test]
fn roundtrip_sip_user_params() {
    roundtrip_sip("sip:+15551234567;cpc=emergency;oli=0@198.51.100.1;user=phone");
}

#[test]
fn roundtrip_tel_basic() {
    roundtrip_tel("tel:+12345678");
}

#[test]
fn roundtrip_tel_with_params() {
    roundtrip_tel("tel:+15551234567;cpc=emergency;oli=0");
}

#[test]
fn roundtrip_uri_sip() {
    roundtrip_uri("sip:alice@example.com;transport=tcp");
}

#[test]
fn roundtrip_uri_tel() {
    roundtrip_uri("tel:+15551234567");
}

#[test]
fn roundtrip_uri_tel_params() {
    roundtrip_uri("tel:+15551234567;cpc=emergency");
}

// ========================================================================
// NG911 production patterns
// ========================================================================

#[test]
fn ng911_with_params() {
    let uri =
        SipUri::parse("sip:+15551234567@sip.bcf.ng911.example.com;participantid=abc123def456")
            .unwrap();
    assert_eq!(uri.scheme(), Some(Scheme::Sip));
    assert_eq!(uri.user(), Some("+15551234567"));
    assert_eq!(
        uri.host()
            .unwrap(),
        &Host::Hostname("sip.bcf.ng911.example.com".into())
    );
    assert_eq!(
        uri.params()
            .len(),
        1
    );
}

#[test]
fn ng911_multiple_userparams() {
    let uri =
        SipUri::parse("sip:+15559876543;cpc=emergency;oli=0@198.51.100.1;user=phone").unwrap();
    assert_eq!(uri.user(), Some("+15559876543"));
    assert_eq!(
        uri.host(),
        Some(&Host::Ipv4(Ipv4Addr::new(198, 51, 100, 1)))
    );
    assert_eq!(
        pairs(uri.user_params()),
        [("cpc", Some("emergency")), ("oli", Some("0"))]
    );
    assert_eq!(uri.param("user"), Some(Some("phone")));
}

#[test]
fn ng911_multiple_params_and_headers() {
    let uri = SipUri::parse("sip:biloxi.com;transport=tcp;method=REGISTER?to=sip:bob%40biloxi.com&from=user%40example.org")
            .unwrap();
    assert_eq!(uri.user(), None);
    assert_eq!(uri.host(), Some(&Host::Hostname("biloxi.com".into())));
    assert_eq!(
        uri.params()
            .len(),
        2
    );
    assert_eq!(uri.param("transport"), Some(Some("tcp")));
    assert_eq!(uri.param("method"), Some(Some("REGISTER")));
    assert_eq!(
        uri.headers()
            .len(),
        2
    );
    // %40 is '@', stays encoded in header values (not in hnv-unreserved)
    assert_eq!(uri.header("to"), Some(Some("sip:bob%40biloxi.com")));
    assert_eq!(uri.header("from"), Some(Some("user%40example.org")));
}

#[test]
fn ng911_ipv4_with_port() {
    let uri = SipUri::parse("sip:1411@198.51.100.2:5061;user=phone").unwrap();
    assert_eq!(uri.user(), Some("1411"));
    assert_eq!(
        uri.host(),
        Some(&Host::Ipv4(Ipv4Addr::new(198, 51, 100, 2)))
    );
    assert_eq!(uri.port(), Some(5061));
    assert_eq!(uri.param("user"), Some(Some("phone")));
}

#[test]
fn ng911_empty_param_value() {
    let uri = SipUri::parse("sip:1411@198.51.100.1;key1=?key2=").unwrap();
    assert_eq!(pairs(uri.params()), [("key1", Some(""))]);
    assert_eq!(pairs(uri.headers()), [("key2", Some(""))]);
}

#[test]
fn ng911_param_without_value_then_header() {
    let uri = SipUri::parse("sip:1411@198.51.100.1;key1?key2").unwrap();
    assert_eq!(pairs(uri.params()), [("key1", None)]);
    assert_eq!(pairs(uri.headers()), [("key2", None)]);
    assert_eq!(uri.to_string(), "sip:1411@198.51.100.1;key1?key2");
}

#[test]
fn ng911_user_param_cpc() {
    let uri = SipUri::parse("sip:5551230001;cpc=emergency@198.51.100.2").unwrap();
    assert_eq!(uri.user(), Some("5551230001"));
    assert_eq!(
        uri.host(),
        Some(&Host::Ipv4(Ipv4Addr::new(198, 51, 100, 2)))
    );
    assert_eq!(pairs(uri.user_params()), [("cpc", Some("emergency"))]);
}

#[test]
fn ng911_tel_without_plus() {
    let tel = TelUri::parse("tel:15551234567").unwrap();
    assert_eq!(tel.number(), Some("15551234567"));
}

#[test]
fn ng911_tel_with_params() {
    let tel = TelUri::parse("tel:+15559871234;cpc=emergency").unwrap();
    assert_eq!(tel.number(), Some("+15559871234"));
    assert_eq!(tel.param("cpc"), Some(Some("emergency")));
}

// ========================================================================
// Sofia-sip additional torture cases
// ========================================================================

#[test]
fn sofia_canonize_method_param() {
    // method=%4D%45%53%53%41%47%45 = METHOD (all unreserved, decode)
    let uri =
        SipUri::parse("sip:pekka.pessi@nokia.com;method=%4D%45%53%53%41%47%45?body=CANNED%20MSG")
            .unwrap();
    assert_eq!(uri.user(), Some("pekka.pessi"));
    assert_eq!(uri.param("method"), Some(Some("MESSAGE")));
    // %20 is space, not unreserved, stays encoded in headers
    assert_eq!(uri.header("body"), Some(Some("CANNED%20MSG")));
}

#[test]
fn sofia_full_with_fragment() {
    let uri = SipUri::parse("sip:user:pass@host:32;param=1?From=foo@bar&To=bar@baz#unf").unwrap();
    assert_eq!(uri.user(), Some("user"));
    assert_eq!(uri.password(), Some("pass"));
    assert_eq!(uri.host(), Some(&Host::Hostname("host".into())));
    assert_eq!(uri.port(), Some(32));
    assert_eq!(uri.param("param"), Some(Some("1")));
    assert_eq!(uri.header("From"), Some(Some("foo%40bar")));
    assert_eq!(uri.header("To"), Some(Some("bar%40baz")));
    assert_eq!(uri.fragment(), Some("unf"));
    assert_eq!(
        uri.to_string(),
        "sip:user:pass@host:32;param=1?From=foo%40bar&To=bar%40baz#unf"
    );
}

#[test]
fn sip_fragment_after_params() {
    let uri = SipUri::parse("sip:alice@example.com;transport=tcp#section").unwrap();
    assert_eq!(uri.param("transport"), Some(Some("tcp")));
    assert_eq!(uri.fragment(), Some("section"));
    assert_eq!(
        uri.to_string(),
        "sip:alice@example.com;transport=tcp#section"
    );
}

#[test]
fn sip_fragment_after_host() {
    let uri = SipUri::parse("sip:example.com#frag").unwrap();
    assert_eq!(uri.host(), Some(&Host::Hostname("example.com".into())));
    assert_eq!(uri.fragment(), Some("frag"));
}

#[test]
fn sip_no_fragment() {
    let uri = SipUri::parse("sip:alice@example.com").unwrap();
    assert_eq!(uri.fragment(), None);
}

#[test]
fn tel_fragment_after_params() {
    let uri = TelUri::parse("tel:+15551234567;cpc=emergency#context").unwrap();
    assert_eq!(uri.number(), Some("+15551234567"));
    assert_eq!(uri.param("cpc"), Some(Some("emergency")));
    assert_eq!(uri.fragment(), Some("context"));
    assert_eq!(uri.to_string(), "tel:+15551234567;cpc=emergency#context");
}

#[test]
fn tel_hash_in_number_not_fragment() {
    // # in tel number is a phonedigit-hex, not a fragment
    let uri = TelUri::parse("tel:*67#").unwrap();
    assert_eq!(uri.number(), Some("*67#"));
    assert_eq!(uri.fragment(), None);
}

#[test]
fn sofia_double_semicolon_in_params() {
    // Empty params between semicolons should be ignored
    let uri = SipUri::parse("sip:u:p@host;user=phone;;").unwrap();
    assert_eq!(uri.param("user"), Some(Some("phone")));
    // The empty params between ;; are ignored
    assert_eq!(
        uri.params()
            .len(),
        1
    );
}

// ========================================================================
// Canonical form verification
// ========================================================================

#[test]
fn canonical_scheme_lowercase() {
    let uri = SipUri::parse("SIP:test@127.0.0.1:55").unwrap();
    assert!(uri
        .to_string()
        .starts_with("sip:"));
}

#[test]
fn canonical_host_lowercase() {
    let uri = SipUri::parse("sip:user@EXAMPLE.COM").unwrap();
    assert_eq!(uri.host(), Some(&Host::Hostname("example.com".into())));
}

#[test]
fn canonical_percent_encoding_uppercase() {
    let uri = SipUri::parse("sip:user@host;param=1%3d%3d1").unwrap();
    // %3d normalized to uppercase %3D
    assert!(uri
        .to_string()
        .contains("%3D%3D1"));
}

#[test]
fn canonical_decode_unreserved_in_user() {
    // %2E is '.', unreserved, should be decoded in user part
    let uri = SipUri::parse("sip:pekka%2Epessi@nokia.com").unwrap();
    assert_eq!(uri.user(), Some("pekka.pessi"));
    assert_eq!(uri.host(), Some(&Host::Hostname("nokia.com".into())));
}

#[test]
fn canonical_percent_encoded_host() {
    // Percent-encoded hostname: %2E is '.', decode unreserved in host
    let uri = SipUri::parse("sip:user@nokia%2Ecom").unwrap();
    assert_eq!(uri.host(), Some(&Host::Hostname("nokia.com".into())));
}

// ========================================================================
// Builder API
// ========================================================================

#[test]
fn builder_sip_uri() {
    let uri = SipUri::new(Host::Ipv4(Ipv4Addr::new(203, 0, 113, 1)))
        .with_scheme(Scheme::Sips)
        .with_user("alice")
        .with_port(5061)
        .with_param("transport", Some("tls"));
    assert_eq!(uri.to_string(), "sips:alice@203.0.113.1:5061;transport=tls");
}

#[test]
fn builder_sip_uri_user_param() {
    let uri = SipUri::new(Host::Ipv4(Ipv4Addr::new(198, 51, 100, 1)))
        .with_user("+15551234567")
        .with_user_param("cpc", Some("emergency"))
        .with_user_param("oli", Some("0"))
        .with_param("user", Some("phone"));
    assert_eq!(
        uri.to_string(),
        "sip:+15551234567;cpc=emergency;oli=0@198.51.100.1;user=phone"
    );
}

#[test]
fn builder_tel_uri() {
    let uri = TelUri::new("+15551234567")
        .with_param("cpc", Some("emergency"))
        .with_param("oli", Some("0"));
    assert_eq!(uri.to_string(), "tel:+15551234567;cpc=emergency;oli=0");
}

// ========================================================================
// Edge cases
// ========================================================================

#[test]
fn sip_uri_no_user_ipv6() {
    let uri = SipUri::parse("sip:[::1]:5060").unwrap();
    assert_eq!(uri.user(), None);
    assert_eq!(uri.host(), Some(&Host::Ipv6(Ipv6Addr::LOCALHOST)));
    assert_eq!(uri.port(), Some(5060));
}

#[test]
fn sip_uri_headers_only() {
    let uri = SipUri::parse("sip:host?Subject=test").unwrap();
    assert_eq!(uri.user(), None);
    assert_eq!(uri.header("Subject"), Some(Some("test")));
}

#[test]
fn tel_local_with_star_hash() {
    let uri = TelUri::parse("tel:*67").unwrap();
    assert_eq!(uri.number(), Some("*67"));
    assert!(!uri.is_global());
}

#[test]
fn tel_visual_separators_preserved() {
    let uri = TelUri::parse("tel:+1.245.623-57").unwrap();
    assert_eq!(uri.number(), Some("+1.245.623-57"));
    assert_eq!(uri.to_string(), "tel:+1.245.623-57");
}

#[test]
fn uri_dispatch_preserves_type() {
    let sip = Uri::parse("sip:alice@example.com").unwrap();
    assert!(sip
        .as_sip()
        .is_some());
    assert!(sip
        .as_tel()
        .is_none());

    let tel = Uri::parse("tel:+15551234567").unwrap();
    assert!(tel
        .as_tel()
        .is_some());
    assert!(tel
        .as_sip()
        .is_none());
}

#[test]
fn param_case_insensitive_lookup() {
    let uri = SipUri::parse("sip:host;Transport=TCP;User=phone").unwrap();
    assert_eq!(uri.param("transport"), Some(Some("TCP")));
    assert_eq!(uri.param("USER"), Some(Some("phone")));
}

#[test]
fn multiple_params_same_name() {
    // RFC doesn't forbid duplicate params; first match wins in our lookup
    let uri = SipUri::parse("sip:host;a=1;a=2").unwrap();
    assert_eq!(uri.param("a"), Some(Some("1")));
    assert_eq!(
        uri.params()
            .len(),
        2
    );
}

#[test]
fn sip_uri_password_deprecated_but_parsed() {
    let uri = SipUri::parse("sip:alice:secret@example.com").unwrap();
    assert_eq!(uri.user(), Some("alice"));
    assert_eq!(uri.password(), Some("secret"));
}

// ========================================================================
// URN tests (RFC 8141 + NG911 production patterns)
// ========================================================================

#[test]
fn urn_service_sos_request_uri() {
    // From production NG911 INVITE Request-URI
    let uri = Uri::parse("urn:service:sos").unwrap();
    let urn = uri
        .as_urn()
        .unwrap();
    assert_eq!(urn.nid(), Some("service"));
    assert_eq!(urn.nss(), Some("sos"));
    assert_eq!(uri.to_string(), "urn:service:sos");
}

#[test]
fn urn_service_sos_with_port() {
    // Seen in To headers: the `:5060` is part of the NSS, not a port.
    let urn = UrnUri::parse("urn:service:sos:5060").unwrap();
    assert_eq!(urn.nss(), Some("sos:5060"));
}

#[test]
fn urn_gsma_imei_in_sip_instance() {
    // From production wireless INVITE Contact +sip.instance
    let urn = UrnUri::parse("urn:gsma:imei:35625207-210812-0").unwrap();
    assert_eq!(urn.nid(), Some("gsma"));
    assert_eq!(urn.nss(), Some("imei:35625207-210812-0"));
    assert_eq!(urn.to_string(), "urn:gsma:imei:35625207-210812-0");
}

#[test]
fn urn_3gpp_ims_service() {
    // From production wireless INVITE P-Preferred-Service
    let urn = UrnUri::parse("urn:urn-7:3gpp-service.ims.icsi.mmtel").unwrap();
    assert_eq!(urn.nid(), Some("urn-7"));
    assert_eq!(urn.nss(), Some("3gpp-service.ims.icsi.mmtel"));
}

#[test]
fn urn_emergency_callid() {
    // From production wireless INVITE Call-Info
    let urn = UrnUri::parse(
        "urn:emergency:callid:a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6:bcf.ng911.example.com",
    )
    .unwrap();
    assert_eq!(urn.nid(), Some("emergency"));
    assert_eq!(
        urn.assigned_name()
            .to_string(),
        "urn:emergency:callid:a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6:bcf.ng911.example.com"
    );
}

#[test]
fn urn_emergency_incidentid() {
    let urn = UrnUri::parse(
        "urn:emergency:incidentid:f1e2d3c4b5a6f7e8d9c0b1a2f3e4d5c6:bcf.ng911.example.com",
    )
    .unwrap();
    assert_eq!(urn.nid(), Some("emergency"));
    assert!(urn
        .nss()
        .unwrap()
        .starts_with("incidentid:"));
}

#[test]
fn urn_nena_callid_wireline() {
    // From production wireline INVITE Call-Info (older NENA format)
    let urn =
        UrnUri::parse("urn:nena:callid:20250101120000001TEST001:bcf1.ng911.example.com").unwrap();
    assert_eq!(urn.nid(), Some("nena"));
    assert!(urn
        .nss()
        .unwrap()
        .starts_with("callid:"));
    assert!(urn
        .nss()
        .unwrap()
        .ends_with("bcf1.ng911.example.com"));
}

#[test]
fn urn_nena_incidentid_wireline() {
    let urn = UrnUri::parse("urn:nena:incidentid:20250101120000002TEST002:bcf1.ng911.example.com")
        .unwrap();
    assert_eq!(urn.nid(), Some("nena"));
    assert!(urn
        .nss()
        .unwrap()
        .starts_with("incidentid:"));
}

#[test]
fn urn_vendor_provider_id() {
    // From production wireline EIDO XML ProviderID
    let urn = UrnUri::parse("urn:example:ng911:lsp:provider1").unwrap();
    assert_eq!(urn.nid(), Some("example"));
    assert_eq!(urn.nss(), Some("ng911:lsp:provider1"));
}

#[test]
fn urn_nena_service_sos() {
    // NENA ESInet internal routing
    let urn = UrnUri::parse("urn:nena:service:sos").unwrap();
    assert_eq!(urn.nid(), Some("nena"));
    assert_eq!(urn.nss(), Some("service:sos"));
}

#[test]
fn urn_nena_service_responder_police() {
    let urn = UrnUri::parse("urn:nena:service:responder.police").unwrap();
    assert_eq!(urn.nss(), Some("service:responder.police"));
}

#[test]
fn urn_uuid_sip_instance() {
    let urn = UrnUri::parse("urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6").unwrap();
    assert_eq!(urn.nid(), Some("uuid"));
    assert_eq!(
        urn.to_string(),
        "urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6"
    );
}

#[test]
fn uri_dispatch_urn_case_insensitive() {
    let uri = Uri::parse("URN:service:sos").unwrap();
    assert!(uri
        .as_urn()
        .is_some());
}

#[test]
fn urn_roundtrip_all_ng911_patterns() {
    let patterns = [
        "urn:service:sos",
        "urn:service:sos.fire",
        "urn:service:sos.police",
        "urn:service:sos.ambulance",
        "urn:nena:service:sos",
        "urn:nena:service:responder.police",
        "urn:gsma:imei:35625207-210812-0",
        "urn:urn-7:3gpp-service.ims.icsi.mmtel",
        "urn:uuid:f81d4fae-7dec-11d0-a765-00a0c91e6bf6",
        "urn:example:ng911:lsp:provider1",
    ];
    for input in patterns {
        let urn = UrnUri::parse(input).expect(input);
        assert_eq!(urn.to_string(), input, "round-trip failed for {input}");
    }
}

#[test]
fn literal_and_escaped_header_octets_compare_equal() {
    let literal =
        SipUri::parse("sip:alice@example.com?From=a@example.org&Subject=%E2%9C%93x").unwrap();
    let escaped =
        SipUri::parse("sip:alice@example.com?From=a%40example.org&Subject=%e2%9c%93%78").unwrap();
    assert_eq!(literal, escaped);
    assert_eq!(
        literal.header("From"),
        Some(Some(
            sip_uri::encoding::encode_header("a@example.org").as_str()
        ))
    );
}

#[test]
fn encode_header_round_trips_through_parse() {
    let replaces = sip_uri::encoding::encode_header("12345@example.com;to-tag=abc;from-tag=def");
    let uri = SipUri::parse(&format!("sip:alice@example.com?Replaces={replaces}")).unwrap();
    assert_eq!(uri.header("Replaces"), Some(Some(replaces.as_str())));
}

#[test]
fn escaped_delimiters_survive_display() {
    for input in [
        "sip:a%3Bb@example.com",
        "sip:alice;x%3Dy=1@example.com",
        "sip:alice;x=a%3Bb@example.com",
        "sip:example.com;maddr=a%40b",
        "sip:example.com?From=a%40b",
    ] {
        let parsed = SipUri::parse(input).unwrap();
        let reparsed = SipUri::parse(&parsed.to_string()).unwrap();
        assert_eq!(reparsed, parsed, "{input} -> {parsed}");
    }
}

#[test]
fn builder_input_cannot_inject_components() {
    let uri = SipUri::new(Host::Hostname("example.com".into()))
        .with_user("+15551234567;cpc=x@evil.example.com")
        .with_user_param("a=b", Some("c;d"))
        .with_password("p:w@x")
        .with_param("x", Some("a;b?c=d"))
        .with_header("Subject", Some("a&b=c#d"));
    let reparsed = SipUri::parse(&uri.to_string()).unwrap();
    assert_eq!(reparsed, uri);
    assert_eq!(
        reparsed
            .params()
            .len(),
        1
    );
    assert_eq!(
        reparsed
            .user_params()
            .len(),
        1
    );
    assert_eq!(
        reparsed
            .headers()
            .len(),
        1
    );
    assert_eq!(reparsed.host(), Some(&Host::Hostname("example.com".into())));
}

#[test]
fn tel_number_cannot_inject_params() {
    let tel = TelUri::new("+1555;x=1");
    assert_eq!(tel.number(), Some("+1555%3Bx=1"));
    let reparsed = TelUri::parse(&tel.to_string()).unwrap();
    assert_eq!(reparsed, tel);
    assert!(reparsed
        .params()
        .is_empty());
}

#[test]
fn hostname_cannot_inject_params() {
    let uri = SipUri::new(Host::Hostname("evil;lr".into()));
    assert_eq!(uri.to_string(), "sip:evil%3Blr");
    let reparsed = SipUri::parse(&uri.to_string()).unwrap();
    assert_eq!(reparsed, uri);
    assert!(reparsed
        .params()
        .is_empty());
}

#[test]
fn hostname_canonizes_like_parsed_host() {
    assert_eq!(Hostname::from("EXAMPLE%2ecom").as_str(), "example.com");
    assert_eq!(Hostname::from("a b%3b").as_str(), "a%20b%3B");
}

#[test]
fn fragments_cannot_inject_components() {
    let sip = SipUri::new(Host::Hostname("example.com".into())).with_fragment("a@b c");
    assert_eq!(sip.fragment(), Some("a%40b%20c"));
    let reparsed = SipUri::parse(&sip.to_string()).unwrap();
    assert_eq!(reparsed, sip);
    assert_eq!(reparsed.user(), None);

    let tel = TelUri::new("+15551234567")
        .with_param("cpc", Some("emergency"))
        .with_fragment("x y#z");
    assert_eq!(tel.fragment(), Some("x%20y#z"));
    let reparsed = TelUri::parse(&tel.to_string()).unwrap();
    assert_eq!(reparsed, tel);

    let urn = UrnUri::new("example", "a?b#c")
        .with_r_component("r?=q")
        .with_f_component("f g");
    assert_eq!(urn.nss(), Some("a%3Fb%23c"));
    assert_eq!(urn.r_component(), Some("r%3F=q"));
    assert_eq!(urn.f_component(), Some("f%20g"));
    let reparsed = UrnUri::parse(&urn.to_string()).unwrap();
    assert_eq!(reparsed, urn);
    assert_eq!(reparsed.q_component(), None);
}

#[test]
fn literal_hash_and_escaped_hash_stay_distinct_in_user() {
    let literal = SipUri::parse("sip:%2A#@example.com").unwrap();
    let escaped = SipUri::parse("sip:*%23@example.com").unwrap();
    assert_eq!(literal.user(), Some("*#"));
    assert_eq!(escaped.user(), Some("*%23"));
    assert_ne!(literal, escaped);
}

#[test]
fn parts_build_what_the_parser_builds() {
    let parsed = SipUri::parse("sip:+15551234567;cpc=emergency@example.com;user=phone").unwrap();
    let mut parts = SipUriParts::default();
    parts.scheme = Some(Scheme::Sip);
    parts.user = Some("+1555%31234567".into());
    parts.user_params = UserParams::new().with("cpc", Some("emergency"));
    parts.host = Some(Host::Hostname("EXAMPLE.COM".into()));
    parts.params = Params::new().with("user", Some("phone"));
    assert_eq!(SipUri::from(parts), parsed);
    assert_eq!(
        SipUri::from(
            parsed
                .clone()
                .into_parts()
        ),
        parsed
    );

    let mut tel = TelUriParts::default();
    tel.number = Some("+15551234567".into());
    assert_eq!(TelUri::from(tel), TelUri::new("+15551234567"));

    let mut urn = UrnUriParts::default();
    urn.nid = Some("SERVICE".into());
    urn.nss = Some("sos".into());
    assert_eq!(
        Uri::from(UrnUri::from(urn)),
        Uri::parse("urn:service:sos").unwrap()
    );
}

#[test]
fn other_uri_new_matches_parsed_other() {
    let parsed = Uri::parse("HTTPS://example.com/a").unwrap();
    let built = OtherUri::new(Some("https"), "//example.com/a").unwrap();
    assert_eq!(parsed, Uri::Other(built));
    assert_eq!(
        OtherUri::new(Some("tel"), "+15551234567"),
        Err(OtherUriError::TypedScheme)
    );
    assert_eq!(
        OtherUri::new(Some("1x"), "y"),
        Err(OtherUriError::InvalidScheme)
    );
}

#[test]
fn equal_uris_hash_equal() {
    let set: std::collections::HashSet<Uri> = [
        "sip:alice@example.com?From=a@example.org",
        "SIP:%61lice@EXAMPLE.com?From=a%40example.org",
        "tel:+15551234567;cpc=emergency",
        "tel:+15551234567;cpc=%65mergency",
        "urn:SERVICE:sos",
        "urn:service:sos",
    ]
    .iter()
    .map(|s| Uri::parse(s).unwrap())
    .collect();
    assert_eq!(set.len(), 3);
}

#[test]
fn builder_canonization_is_idempotent() {
    let uri = SipUri::parse("sip:%61lice;x=a%3Bb@example.com;p=%41").unwrap();
    let rebuilt = SipUri::new(
        uri.host()
            .unwrap()
            .clone(),
    )
    .with_user(
        uri.user()
            .unwrap(),
    )
    .with_user_params(
        uri.user_params()
            .clone(),
    )
    .with_param(
        "p",
        uri.param("p")
            .flatten(),
    );
    assert_eq!(rebuilt, uri);
}
