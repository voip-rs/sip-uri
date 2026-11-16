use sip_uri::{
    HeaderMask, HostMask, OpaqueMask, Redaction, SipUri, TelUri, Uri, UriParse, UriRedact, UrnUri,
    UserMask,
};

fn sip(s: &str) -> SipUri {
    SipUri::parse(s).unwrap()
}

#[test]
fn default_masks_whole_userinfo() {
    let uri = sip("sips:+15551234567;cpc=emergency:secret@example.com:5061;user=phone?Subject=x");
    assert_eq!(
        uri.redacted(&Redaction::default())
            .to_string(),
        "sips:***@example.com:5061;user=phone?Subject=***"
    );
}

#[test]
fn header_values_are_masked_unless_shown() {
    let uri = sip("sip:example.com?P-Asserted-Identity=sip:%2B15551234567%40example.com&Flag&E=");
    let render = |how: Redaction| {
        uri.redacted(&how)
            .to_string()
    };
    assert_eq!(Redaction::default().header_mask(), HeaderMask::Values);
    assert_eq!(
        render(Redaction::default()),
        "sip:example.com?P-Asserted-Identity=***&Flag&E=***"
    );
    assert_eq!(
        render(Redaction::default().headers(HeaderMask::Visible)),
        uri.to_string()
    );
    assert_eq!(
        render(Redaction::default().drop_headers()),
        render(Redaction::default().headers(HeaderMask::Dropped))
    );
    assert_eq!(
        render(Redaction::default().drop_headers()),
        "sip:example.com"
    );
}

#[test]
fn masks_param_ignores_ascii_case() {
    let how = Redaction::default().params(["ParticipantId"]);
    assert!(how.masks_param("participantid"));
    assert!(how.masks_param("PARTICIPANTID"));
    assert!(!how.masks_param("transport"));
    assert!(!Redaction::default().masks_param("participantid"));
}

#[test]
fn masked_param_names_match_however_they_are_spelled() {
    let uri = sip("sip:example.com;a%20b=secret;token=secret2");
    let how = Redaction::default().params(["a b", "tok%65n"]);
    assert_eq!(
        uri.redacted(&how)
            .to_string(),
        "sip:example.com;a%20b=***;token=***"
    );
    let how = Redaction::default().params(["a;b", "%73ession"]);
    for spelling in ["a;b", "a%3Bb", "%73ession", "session", "SESSION"] {
        assert!(how.masks_param(spelling), "{spelling}");
    }
    assert!(!how.masks_param(""));
    let user = sip("sip:alice;a,b=secret@example.com");
    let how = Redaction::default()
        .user(UserMask::Visible)
        .params(["a,b"]);
    assert_eq!(
        user.redacted(&how)
            .to_string(),
        "sip:alice;a,b=***@example.com"
    );
}

#[test]
fn user_mask_reads_back() {
    assert_eq!(Redaction::default().user_mask(), UserMask::Full);
    assert_eq!(
        Redaction::default()
            .user(UserMask::KeepLast(4))
            .user_mask(),
        UserMask::KeepLast(4)
    );
}

#[test]
fn default_leaves_uri_without_userinfo_unchanged() {
    let uri = sip("sip:example.com;transport=tcp");
    assert_eq!(
        uri.redacted(&Redaction::default())
            .to_string(),
        uri.to_string()
    );
}

#[test]
fn visible_user_still_masks_password() {
    let uri = sip("sip:alice:secret@example.com");
    assert_eq!(
        uri.redacted(&Redaction::default().user(UserMask::Visible))
            .to_string(),
        "sip:alice:***@example.com"
    );
}

#[test]
fn keep_last_masks_only_digits() {
    let how = Redaction::default().user(UserMask::KeepLast(4));
    assert_eq!(
        sip("sip:+1-555-123-4567@example.com")
            .redacted(&how)
            .to_string(),
        "sip:+x-xxx-xxx-4567@example.com"
    );
    assert_eq!(
        sip("sip:12@example.com")
            .redacted(&how)
            .to_string(),
        "sip:12@example.com"
    );
}

#[test]
fn keep_last_leaves_escapes_intact() {
    let how = Redaction::default().user(UserMask::KeepLast(4));
    assert_eq!(
        sip("sip:%2B15551234567@example.com")
            .redacted(&how)
            .to_string(),
        "sip:%2Bxxxxxxx4567@example.com"
    );
    assert_eq!(
        sip("sip:%2B1234@example.com")
            .redacted(&Redaction::default().user(UserMask::KeepLast(2)))
            .to_string(),
        "sip:%2Bxx34@example.com"
    );
}

#[test]
fn headers_and_named_params_can_be_masked() {
    let uri = sip("sip:alice@example.com;participantid=abc;user=phone?Replaces=x%40y");
    let how = Redaction::default()
        .user(UserMask::Visible)
        .drop_headers()
        .params(["ParticipantId"]);
    assert_eq!(
        uri.redacted(&how)
            .to_string(),
        "sip:alice@example.com;participantid=***;user=phone"
    );
}

#[test]
fn tel_number_is_masked_by_default() {
    let tel = TelUri::parse("tel:+15551234567;cpc=emergency").unwrap();
    assert_eq!(
        tel.redacted(&Redaction::default())
            .to_string(),
        "tel:***;cpc=emergency"
    );
    assert_eq!(
        tel.redacted(&Redaction::default().user(UserMask::KeepLast(4)))
            .to_string(),
        "tel:+xxxxxxx4567;cpc=emergency"
    );
}

#[test]
fn params_carrying_a_number_are_masked_as_the_number() {
    let tel = TelUri::parse(
        "tel:5551234;phone-context=+1555;ext=1234;isub=9999;rn=+15559876543;cic=+15550000",
    )
    .unwrap();
    assert_eq!(
        tel.redacted(&Redaction::default())
            .to_string(),
        "tel:***;phone-context=+1555;ext=***;isub=***;rn=+15559876543;cic=+15550000"
    );
    assert_eq!(
        tel.redacted(&Redaction::default().user(UserMask::KeepLast(2)))
            .to_string(),
        "tel:xxxxx34;phone-context=+1555;ext=xx34;isub=xx99;rn=+15559876543;cic=+15550000"
    );
    let domain = TelUri::parse("tel:5551234;phone-context=example.com").unwrap();
    assert_eq!(
        domain
            .redacted(&Redaction::default())
            .to_string(),
        "tel:***;phone-context=example.com"
    );
    let sip = sip("sip:+15551234567;ext=1234@example.com;ext=1234");
    assert_eq!(
        sip.redacted(&Redaction::default().user(UserMask::KeepLast(2)))
            .to_string(),
        "sip:+xxxxxxxxx67;ext=xx34@example.com;ext=1234"
    );
}

#[test]
fn uri_dispatches_to_each_kind() {
    let sip = Uri::parse("sip:alice@example.com").unwrap();
    assert_eq!(
        sip.redacted(&Redaction::default())
            .to_string(),
        "sip:***@example.com"
    );
    let urn = Uri::parse("urn:gsma:imei:TEST-0000-0000-0").unwrap();
    let masked = "urn:gsma:***";
    assert_eq!(
        urn.redacted(&Redaction::default())
            .to_string(),
        masked
    );
    let urn = urn
        .as_urn()
        .unwrap();
    assert_eq!(
        urn.redacted(&Redaction::default())
            .to_string(),
        masked
    );
    let visible = Redaction::default().urn(OpaqueMask::Visible);
    assert_eq!(visible.urn_mask(), OpaqueMask::Visible);
    assert_eq!(
        urn.redacted(&visible)
            .to_string(),
        urn.to_string()
    );
}

#[test]
fn public_urn_kinds_are_shown() {
    let shown = [
        "urn:Service:sos",
        "urn:nena:service:responder.police",
        "urn:nena:service",
        "urn:emergency:uid:callid:TEST1:bcf.example.com",
        "urn:emergency:uid:incidentid:TEST2:psap.example.com",
        "urn:emergency:service:sos.psap",
        "urn:emergency:service:responder.police",
    ];
    for text in shown {
        let urn = Uri::parse(text).unwrap();
        assert_eq!(
            urn.redacted(&Redaction::default())
                .to_string(),
            urn.to_string(),
            "{text}"
        );
    }
    let masked = [
        ("urn:nena:companyid:TEST3", "urn:nena:***"),
        ("urn:nena:serviceX:TEST4", "urn:nena:***"),
        ("urn:emergency:other:TEST5", "urn:emergency:***"),
        ("urn:emergency:serviceX:TEST6", "urn:emergency:***"),
        ("urn:example:TEST-1", "urn:example:***"),
    ];
    for (text, expected) in masked {
        assert_eq!(
            Uri::parse(text)
                .unwrap()
                .redacted(&Redaction::default())
                .to_string(),
            expected
        );
    }

    let listed = Redaction::default().public_urns(["EXAMPLE", "nena:companyid"]);
    let example = UrnUri::parse("urn:example:TEST-1").unwrap();
    let company = UrnUri::parse("urn:nena:companyid:TEST3").unwrap();
    let sos = UrnUri::parse("urn:service:sos").unwrap();
    assert!(listed.is_public_urn(&example));
    assert!(listed.is_public_urn(&company));
    assert!(!listed.is_public_urn(&sos));
    assert_eq!(
        sos.redacted(&listed)
            .to_string(),
        "urn:service:***"
    );
}

#[test]
fn caller_uris_parsed_as_other_are_masked_by_default() {
    let inputs = [
        ("<sip:+15551234567@example.com>", "***"),
        ("+15551234567@example.com", "***"),
        ("\"Alice\" <sip:+15551234567@example.com>", "***"),
        (
            "http://alice:secret@example.com/",
            "http://***@example.com/",
        ),
        (
            "https://u@[2001:db8::1]:8443/adr/1?caller=+15551234567#top",
            "https://***@[2001:db8::1]:8443/adr/1?***#top",
        ),
        ("https://example.com?token=x", "https://example.com?***"),
        ("https://example.com/a#b?c", "https://example.com/a#b?c"),
        ("mailto:alice@example.com", "mailto:***"),
        ("cid:part1@example.com", "cid:***"),
    ];
    let visible = Redaction::default().other(OpaqueMask::Visible);
    assert_eq!(visible.other_mask(), OpaqueMask::Visible);
    for (input, masked) in inputs {
        let uri = Uri::parse(input).unwrap();
        let other = uri
            .as_other()
            .unwrap();
        assert_eq!(
            uri.redacted(&Redaction::default())
                .to_string(),
            masked,
            "{input}"
        );
        assert_eq!(
            other
                .redacted(&Redaction::default())
                .to_string(),
            masked
        );
        assert_eq!(
            uri.redacted(&visible)
                .to_string(),
            uri.to_string()
        );
    }
}

#[test]
fn off_grammar_positions_are_shown_by_default_and_masked_on_request() {
    let host_number = sip("sip:5551234567;transport=udp");
    assert_eq!(
        host_number
            .redacted(&Redaction::default())
            .to_string(),
        "sip:5551234567;transport=udp"
    );
    let masked_host = Redaction::default().host(HostMask::Full);
    assert_eq!(masked_host.host_mask(), HostMask::Full);
    assert_eq!(
        sip("sip:alice@example.com:5060")
            .redacted(&masked_host)
            .to_string(),
        "sip:***@***:5060"
    );

    let nid_number = Uri::parse("urn:5551234567").unwrap();
    assert_eq!(
        nid_number
            .redacted(&Redaction::default())
            .to_string(),
        "urn:5551234567"
    );
    let full = Redaction::default()
        .urn(OpaqueMask::Full)
        .other(OpaqueMask::Full);
    assert_eq!(
        nid_number
            .redacted(&full)
            .to_string(),
        "urn:***"
    );

    let scheme_user = Uri::parse("alice:secret@example.com").unwrap();
    assert_eq!(
        scheme_user
            .redacted(&Redaction::default())
            .to_string(),
        "alice:***"
    );
    assert_eq!(
        scheme_user
            .redacted(&full)
            .to_string(),
        "***"
    );
}

#[test]
fn debug_of_a_rendering_is_redacted() {
    let uri = sip("sip:+15551234567:pw@example.com");
    assert_eq!(
        format!("{:?}", uri.redacted(&Redaction::default())),
        r#"Redacted(sip:***@example.com)"#
    );
}
