use sip_uri::{
    Component, Host, ParseWarning, Parsed, SipUri, TelUri, Uri, UriParse, UrnUri, WarningCode,
    WarningKind,
};

fn codes(warnings: &[ParseWarning]) -> Vec<(Component, WarningCode)> {
    warnings
        .iter()
        .map(|w| (w.component, w.code))
        .collect()
}

fn sip(input: &str) -> (SipUri, Vec<ParseWarning>) {
    let parsed = SipUri::parse_with_warnings(input).unwrap();
    let reparsed = SipUri::parse(
        &parsed
            .value
            .to_string(),
    )
    .unwrap();
    assert_eq!(reparsed, parsed.value, "round-trip of {input:?}");
    assert_eq!(SipUri::parse(input).unwrap(), parsed.value);
    (parsed.value, parsed.warnings)
}

fn only(warnings: &[ParseWarning], component: Component, code: WarningCode) -> ParseWarning {
    assert_eq!(codes(warnings), [(component, code)], "{warnings:?}");
    warnings[0].clone()
}

#[test]
fn parsed_destructures_and_builds_outside_the_crate() {
    let Parsed { value, warnings } = SipUri::parse_with_warnings("sip:example.com:+5060").unwrap();
    assert_eq!(
        codes(&warnings),
        [(Component::Port, WarningCode::SignedPort)]
    );
    let rebuilt = Parsed::new(value.clone(), warnings.clone()).map(Uri::Sip);
    assert_eq!(rebuilt, Parsed::new(Uri::Sip(value), warnings));
}

#[test]
fn conformant_inputs_raise_nothing() {
    for input in [
        "sip:alice@example.com",
        "sips:alice@example.com:5061;transport=tls",
        "sip:+15551234567;cpc=emergency;oli=0@198.51.100.1;user=phone",
        "sip:1411@[2001:db8::1]:5061;user=phone",
        "sip:sip.bcf.qc.core.ng.example.com;participantid=2;user=phone",
        "sip:biloxi.com;transport=tcp;method=REGISTER?to=sip:bob%40biloxi.com&from=user%40example.org",
        "sip:pekka.pessi@example.com;method=%4D%45%53%53%41%47%45?body=CANNED%20MSG",
        "sip:1411@198.51.100.2;key1?key2=",
        "sip:user@example.com.",
    ] {
        let parsed = SipUri::parse_with_warnings(input).unwrap();
        assert!(parsed.warnings.is_empty(), "{input}: {:?}", parsed.warnings);
    }
    for input in [
        "tel:+15551234567;cpc=emergency;oli=0",
        "tel:1411;phone-context=example.com",
        "tel:+1.555-123-4567",
    ] {
        let parsed = TelUri::parse_with_warnings(input).unwrap();
        assert!(
            parsed
                .warnings
                .is_empty(),
            "{input}: {:?}",
            parsed.warnings
        );
    }
    for input in [
        "urn:service:sos",
        "urn:nena:callid:20250101120000001TEST001:bcf1.ng911.example.com",
        "urn:example:foo?+resolve?=query#frag",
        "urn:example:foo#",
    ] {
        let parsed = UrnUri::parse_with_warnings(input).unwrap();
        assert!(
            parsed
                .warnings
                .is_empty(),
            "{input}: {:?}",
            parsed.warnings
        );
    }
    for input in ["https://example.com/photo.jpg", "cid:abc@example.com"] {
        let parsed = Uri::parse_with_warnings(input).unwrap();
        assert!(
            parsed
                .warnings
                .is_empty(),
            "{input}: {:?}",
            parsed.warnings
        );
    }
}

#[test]
fn invalid_user_char() {
    let (uri, w) = sip("sip:a b@example.com");
    assert_eq!(uri.user(), Some("a%20b"));
    let w = only(&w, Component::User, WarningCode::InvalidChar);
    assert_eq!(w.position, Some(5));
    assert_eq!(w.kind(), WarningKind::Recovered);
}

#[test]
fn malformed_user_escape() {
    let (uri, w) = sip("sip:us%zzer@example.com");
    assert_eq!(uri.user(), Some("us%25zzer"));
    only(&w, Component::User, WarningCode::MalformedEscape);
}

#[test]
fn hash_in_user() {
    let (uri, w) = sip("sip:#**00**#;foo=/bar@198.51.100.1");
    assert_eq!(uri.user(), Some("#**00**#"));
    let w = only(&w, Component::User, WarningCode::InvalidChar);
    assert_eq!(w.position, Some(4));
}

#[test]
fn invalid_password_char() {
    let (uri, w) = sip("sip:alice:p w@example.com");
    assert_eq!(uri.password(), Some("p%20w"));
    only(&w, Component::Password, WarningCode::InvalidChar);
}

#[test]
fn password_without_user() {
    let (uri, w) = sip("sip::secret@example.com");
    assert_eq!(uri.user(), None);
    assert_eq!(uri.password(), Some("secret"));
    only(&w, Component::User, WarningCode::PasswordWithoutUser);
}

#[test]
fn header_shaped_user() {
    let (uri, w) = sip("sip:example.com?From=a@example.org");
    assert_eq!(uri.user(), Some("example.com?From=a"));
    let w = only(&w, Component::User, WarningCode::HeaderShapedUser);
    assert_eq!(w.position, Some(15));
}

#[test]
fn signed_port() {
    let (uri, w) = sip("sip:example.com:+5060");
    assert_eq!(uri.port(), Some(5060));
    let w = only(&w, Component::Port, WarningCode::SignedPort);
    assert_eq!(w.position, Some(16));
}

#[test]
fn empty_port() {
    let (uri, w) = sip("sip:alice@198.51.100.1:");
    assert_eq!(uri.port(), None);
    let w = only(&w, Component::Port, WarningCode::EmptyPort);
    assert_eq!(w.kind(), WarningKind::Lost);
}

#[test]
fn empty_param_name() {
    let (uri, w) = sip("sip:example.com;=v");
    assert_eq!(uri.param(""), Some(Some("v")));
    only(&w, Component::Param, WarningCode::EmptyName);
}

#[test]
fn empty_user_param_name() {
    let (_, w) = sip("sip:alice;=v@example.com");
    only(&w, Component::UserParam, WarningCode::EmptyName);
}

#[test]
fn empty_param_segment() {
    let (uri, w) = sip("sip:example.com;user=phone;;");
    assert_eq!(
        uri.params()
            .len(),
        1
    );
    assert_eq!(
        codes(&w),
        [
            (Component::Param, WarningCode::EmptySegment),
            (Component::Param, WarningCode::EmptySegment),
        ]
    );
    assert!(w
        .iter()
        .all(|w| w.kind() == WarningKind::Lost));
}

#[test]
fn param_extension_chars() {
    let (uri, w) = sip("sip:alice@example.com;maddr=a@b");
    assert_eq!(uri.param("maddr"), Some(Some("a%40b")));
    only(&w, Component::Param, WarningCode::InvalidChar);
}

#[test]
fn invalid_header_char() {
    let (uri, w) = sip("sip:alice@example.com?Subject=a b");
    assert_eq!(uri.header("Subject"), Some(Some("a%20b")));
    only(&w, Component::Header, WarningCode::InvalidChar);
}

#[test]
fn malformed_header_escape() {
    let (_, w) = sip("sip:alice@example.com?a=%");
    only(&w, Component::Header, WarningCode::MalformedEscape);
}

#[test]
fn empty_header_segment() {
    let (uri, w) = sip("sip:alice@example.com?a=1&&b=2");
    assert_eq!(
        uri.headers()
            .len(),
        2
    );
    only(&w, Component::Header, WarningCode::EmptySegment);
}

#[test]
fn invalid_host_labels() {
    for input in [
        "sip:-bad.example.com",
        "sip:bad-.example.com",
        "sip:a..example.com",
    ] {
        let (_, w) = sip(input);
        only(&w, Component::Host, WarningCode::InvalidHostLabel);
    }
}

#[test]
fn numeric_toplabel() {
    let (uri, w) = sip("sip:999.1.1.1");
    assert_eq!(uri.host(), Some(&Host::Hostname("999.1.1.1".into())));
    let w = only(&w, Component::Host, WarningCode::NumericToplabel);
    assert_eq!(w.position, Some(12));
}

#[test]
fn escaped_host() {
    let (uri, w) = sip("sip:alice@example%2Ecom");
    assert_eq!(uri.host(), Some(&Host::Hostname("example.com".into())));
    only(&w, Component::Host, WarningCode::EscapedHost);
}

#[test]
fn sip_fragment() {
    let (uri, w) = sip("sip:alice@example.com#frag");
    assert_eq!(uri.fragment(), Some("frag"));
    only(&w, Component::Fragment, WarningCode::UnexpectedFragment);

    let (uri, w) = sip("sip:alice@example.com#");
    assert_eq!(uri.fragment(), None);
    only(&w, Component::Fragment, WarningCode::EmptyFragment);
}

#[test]
fn tel_missing_phone_context() {
    let parsed = TelUri::parse_with_warnings("tel:911").unwrap();
    assert_eq!(
        parsed
            .value
            .number()
            .unwrap(),
        "911"
    );
    only(
        &parsed.warnings,
        Component::Number,
        WarningCode::MissingPhoneContext,
    );
}

#[test]
fn tel_param_charset() {
    let parsed = TelUri::parse_with_warnings("tel:+15551234567;a_b=1").unwrap();
    only(&parsed.warnings, Component::Param, WarningCode::InvalidChar);

    let parsed = TelUri::parse_with_warnings("tel:+15551234567;=1").unwrap();
    only(&parsed.warnings, Component::Param, WarningCode::EmptyName);
}

#[test]
fn tel_fragment() {
    let parsed = TelUri::parse_with_warnings("tel:+15551234567;cpc=emergency#x").unwrap();
    only(
        &parsed.warnings,
        Component::Fragment,
        WarningCode::UnexpectedFragment,
    );
}

#[test]
fn urn_empty_components() {
    let parsed = UrnUri::parse_with_warnings("urn:example:foo?+").unwrap();
    assert_eq!(
        parsed
            .value
            .r_component(),
        Some("")
    );
    only(
        &parsed.warnings,
        Component::RComponent,
        WarningCode::EmptyComponent,
    );

    let parsed = UrnUri::parse_with_warnings("urn:example:foo?=").unwrap();
    only(
        &parsed.warnings,
        Component::QComponent,
        WarningCode::EmptyComponent,
    );
}

#[test]
fn urn_component_charsets() {
    let parsed = UrnUri::parse_with_warnings("urn:example:foo?+a b").unwrap();
    only(
        &parsed.warnings,
        Component::RComponent,
        WarningCode::InvalidChar,
    );

    let parsed = UrnUri::parse_with_warnings("urn:example:foo?+a#b#c").unwrap();
    assert_eq!(
        parsed
            .value
            .r_component(),
        Some("a")
    );
    assert_eq!(
        parsed
            .value
            .f_component(),
        Some("b#c")
    );
    let w = only(
        &parsed.warnings,
        Component::FComponent,
        WarningCode::InvalidChar,
    );
    assert_eq!(w.position, Some(20));
    assert_eq!(
        parsed
            .value
            .to_string(),
        "urn:example:foo?+a#b#c"
    );
}

#[test]
fn other_text_outside_rfc3986_warns() {
    let input = "http://example.com/a b\r\nVia: x";
    let parsed = Uri::parse_with_warnings(input).unwrap();
    let w = only(&parsed.warnings, Component::Other, WarningCode::InvalidChar);
    assert_eq!(
        w.position,
        Some(
            input
                .find(' ')
                .unwrap()
        )
    );
    assert!(Uri::parse_strict(input).is_err());

    let parsed = Uri::parse_with_warnings("http://example.com/a%zz").unwrap();
    only(
        &parsed.warnings,
        Component::Other,
        WarningCode::MalformedEscape,
    );

    for conformant in [
        "http://[2001:db8::1]:8080/a%20b?q=1&r=a,b;c#f",
        "data:,x",
        "mailto:a@example.com",
    ] {
        assert!(Uri::parse_strict(conformant).is_ok(), "{conformant}");
    }
}

#[test]
fn uri_invalid_scheme() {
    for (input, held) in [
        ("<sip:alice@example.com>", "%3Csip:alice@example.com%3E"),
        (" sip:alice@example.com", "%20sip:alice@example.com"),
    ] {
        let parsed = Uri::parse_with_warnings(input).unwrap();
        assert_eq!(
            parsed
                .value
                .as_other()
                .map(|o| o.as_str()),
            Some(held)
        );
        let w = only(
            &parsed.warnings,
            Component::Scheme,
            WarningCode::InvalidScheme,
        );
        assert_eq!(w.position, Some(0));
    }
}

#[test]
fn unreadable_scheme_prefix_is_the_user_part() {
    let (uri, w) = sip("<sip:+15551234567;cpc=ordinary@example.com:5060;transport=tcp>");
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.user(), Some("%3Csip%3A+15551234567"));
    assert_eq!(uri.user_param("cpc"), Some(Some("ordinary")));
    assert_eq!(uri.password(), None);
    assert_eq!(uri.host(), Some(&Host::Hostname("example.com".into())));
    assert_eq!(uri.port(), Some(5060));
    assert_eq!(uri.param("transport"), Some(Some("tcp%3E")));
    assert_eq!(w[0].code, WarningCode::InvalidScheme);
    assert_eq!(w[0].position, Some(0));
    assert_eq!(SipUri::parse(&uri.to_string()).unwrap(), uri);

    let (uri, w) = sip("  sip:alice@example.com  ");
    assert_eq!(uri.user(), Some("%20%20sip%3Aalice"));
    assert_eq!(uri.password(), None);
    assert_eq!(w[0].code, WarningCode::InvalidScheme);
    assert_eq!(SipUri::parse(&uri.to_string()).unwrap(), uri);
}

#[test]
fn missing_scheme_keeps_the_password() {
    let parsed = SipUri::parse_with_warnings("alice:5060;x@example.com").unwrap();
    let uri = parsed.value;
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.user(), Some("alice"));
    assert_eq!(uri.password(), Some("5060%3Bx"));
    assert_eq!(parsed.warnings[0].code, WarningCode::MissingScheme);
    assert_eq!(
        SipUri::parse("alice:pw@example.com"),
        Err(sip_uri::ParseError::SchemeMismatch)
    );
}

#[test]
fn uri_forwards_scheme_warnings() {
    let parsed = Uri::parse_with_warnings("sip:example.com:+5060").unwrap();
    only(&parsed.warnings, Component::Port, WarningCode::SignedPort);
}

#[test]
fn standalone_host() {
    let parsed = Host::parse_with_warnings("-bad.example.com").unwrap();
    only(
        &parsed.warnings,
        Component::Host,
        WarningCode::InvalidHostLabel,
    );
    assert!(!Host::parse_with_warnings("2001:db8::1")
        .unwrap()
        .has_warnings());
}

#[test]
fn display_names_component_and_position() {
    let (_, w) = sip("sip:a b@example.com");
    assert_eq!(w[0].to_string(), "user: invalid character at byte 5");
}

#[test]
fn unreadable_components_become_absent() {
    for (input, component, code) in [
        ("sip:[::1", Component::Host, WarningCode::InvalidIpv6),
        ("sip:[zz::1]", Component::Host, WarningCode::InvalidIpv6),
        (
            "sip:[198.51.100.1]",
            Component::Host,
            WarningCode::InvalidIpv6,
        ),
        ("sip:", Component::Host, WarningCode::MissingHost),
        ("sip::5060", Component::Host, WarningCode::MissingHost),
        ("sip:host:99999", Component::Port, WarningCode::InvalidPort),
        ("sip:[::1]x", Component::Host, WarningCode::TrailingContent),
        ("sip:h;a=b c", Component::Param, WarningCode::InvalidChar),
        (
            "sip:u;a b@h",
            Component::UserParam,
            WarningCode::InvalidChar,
        ),
        (
            "sip:my_host.example.com",
            Component::Host,
            WarningCode::InvalidChar,
        ),
        ("sip:h?foo", Component::Header, WarningCode::MissingValue),
        ("sip:h?=v", Component::Header, WarningCode::EmptyName),
        ("sip:;cpc=x@h", Component::User, WarningCode::EmptyUser),
        ("sip:@h", Component::User, WarningCode::EmptyUserinfo),
    ] {
        let (_, w) = sip(input);
        assert!(
            w.iter()
                .any(|w| w.component == component && w.code == code),
            "{input}: expected {component:?} {code:?} in {w:?}"
        );
    }
}

#[test]
fn unreadable_tel_and_urn_components() {
    let parsed = TelUri::parse_with_warnings("tel:+1 555 123 4567").unwrap();
    assert_eq!(
        parsed
            .value
            .number(),
        Some("+1%20555%20123%204567")
    );
    only(
        &parsed.warnings,
        Component::Number,
        WarningCode::InvalidChar,
    );

    let parsed = UrnUri::parse_with_warnings("urn:foo").unwrap();
    assert_eq!(
        parsed
            .value
            .nid(),
        Some("foo")
    );
    assert_eq!(
        parsed
            .value
            .nss(),
        None
    );
    only(&parsed.warnings, Component::Nss, WarningCode::MissingNss);

    let parsed = UrnUri::parse_with_warnings("urn::sos").unwrap();
    assert_eq!(
        parsed
            .value
            .nid(),
        None
    );
    assert_eq!(
        parsed
            .value
            .to_string(),
        "urn::sos"
    );
    only(&parsed.warnings, Component::Nid, WarningCode::MissingNid);
}

#[test]
fn missing_scheme_keeps_user_and_host() {
    let (uri, w) = sip("alice@example.com:5060");
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.user(), Some("alice"));
    assert_eq!(uri.host(), Some(&Host::Hostname("example.com".into())));
    assert_eq!(uri.port(), Some(5060));
    only(&w, Component::Scheme, WarningCode::MissingScheme);

    let parsed = Uri::parse_with_warnings("alice@example.com").unwrap();
    assert_eq!(
        parsed
            .value
            .as_other()
            .map(|o| o.as_str()),
        Some("alice@example.com")
    );
    only(
        &parsed.warnings,
        Component::Scheme,
        WarningCode::MissingScheme,
    );
}

#[test]
fn strict_rejects_what_lenient_warns_about() {
    assert_eq!(
        SipUri::parse_strict("sip:example.com:+5060"),
        Err(sip_uri::ParseError::NonConformant(
            SipUri::parse_with_warnings("sip:example.com:+5060")
                .unwrap()
                .warnings[0]
                .clone()
        ))
    );
    assert!(SipUri::parse_strict("sip:alice@example.com").is_ok());
    assert!(TelUri::parse_strict("tel:911").is_err());
    assert!(UrnUri::parse_strict("urn:x:foo").is_err());
    assert!(Uri::parse_strict("<sip:alice@example.com>").is_err());
    assert!(Host::parse_strict("example.com:5060").is_err());
    assert_eq!(SipUri::parse_strict(""), Err(sip_uri::ParseError::Empty));
    assert_eq!(
        SipUri::parse_strict("tel:+15551234567"),
        Err(sip_uri::ParseError::SchemeMismatch)
    );
}

#[test]
fn errors_never_quote_input() {
    let err = SipUri::parse_strict("sip:+15551234567@example.com:+5060").unwrap_err();
    assert_eq!(
        err.to_string(),
        "non-conformant URI: port: signed port at byte 29"
    );
}

#[test]
fn other_scheme_is_lowercased() {
    let upper = Uri::parse("HTTPS://example.com/Photo.jpg").unwrap();
    let lower = Uri::parse("https://example.com/Photo.jpg").unwrap();
    assert_eq!(upper, lower);
    assert_eq!(upper.scheme(), Some("https"));
    assert_eq!(upper.to_string(), "https://example.com/Photo.jpg");
}

#[test]
fn built_hostname_equals_parsed() {
    let built = SipUri::new(Host::Hostname("EXAMPLE.COM".into())).with_user("alice");
    let parsed = SipUri::parse("sip:alice@example.com").unwrap();
    assert_eq!(built, parsed);
}

/// `Parsed::warnings` promises input order, and nothing sorts it: each
/// parser has to check components left to right.
#[test]
fn warnings_come_in_input_order() {
    fn in_order(warnings: &[ParseWarning]) -> bool {
        warnings
            .windows(2)
            .all(|w| w[0].position <= w[1].position)
    }
    for input in ["tel:12 3;a b=1#x y", "tel:;a b=1#x%zz"] {
        let parsed = TelUri::parse_with_warnings(input).unwrap();
        assert!(in_order(&parsed.warnings), "{input}: {:?}", parsed.warnings);
    }
    let input = "sip:a b;c d:p w@ex ample.com:x;y z?h i#f g";
    let parsed = SipUri::parse_with_warnings(input).unwrap();
    assert!(in_order(&parsed.warnings), "{input}: {:?}", parsed.warnings);
    let Err(sip_uri::ParseError::NonConformant(first)) = SipUri::parse_strict(input) else {
        panic!("strict parse accepted {input}");
    };
    assert_eq!(first.component, Component::User);
}
