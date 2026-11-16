use sip_uri::sip_uri_types::Headers;
use sip_uri::{SipUri, TelUri, Uri, UriEquivalence, UriHeaderEquivalence, UriParse, UrnUri};

fn check<T: UriParse + UriEquivalence>(a: &str, b: &str, expected: bool) {
    let (a_uri, b_uri) = (T::parse(a).unwrap(), T::parse(b).unwrap());
    assert!(a_uri.equivalent(&a_uri), "{a} not equivalent to itself");
    assert_eq!(a_uri.equivalent(&b_uri), expected, "{a} vs {b}");
    assert_eq!(b_uri.equivalent(&a_uri), expected, "{b} vs {a}");
}

/// RFC 3261 §19.1.4's equivalent sets, hosts moved to example domains.
#[test]
fn sip_rfc_equivalent_examples() {
    for set in [
        &[
            "sip:%61lice@example.com;transport=TCP",
            "sip:alice@ExAmPlE.CoM;Transport=tcp",
        ][..],
        &[
            "sip:carol@example.org",
            "sip:carol@example.org;newparam=5",
            "sip:carol@example.org;security=on",
        ],
        &[
            "sip:example.net;transport=tcp;method=REGISTER?to=sip:bob%40example.net",
            "sip:example.net;method=REGISTER;transport=tcp?to=sip:bob%40example.net",
        ],
        &[
            "sip:alice@example.com?subject=project%20x&priority=urgent",
            "sip:alice@example.com?priority=urgent&subject=project%20x",
        ],
    ] {
        for a in set {
            for b in set {
                check::<SipUri>(a, b, true);
            }
        }
    }
}

/// RFC 3261 §19.1.4's non-equivalent pairs, hosts moved to example domains.
#[test]
fn sip_rfc_non_equivalent_examples() {
    for (a, b) in [
        (
            "SIP:ALICE@example.com;Transport=udp",
            "sip:alice@example.com;Transport=UDP",
        ),
        ("sip:bob@example.net", "sip:bob@example.net:5060"),
        ("sip:bob@example.net", "sip:bob@example.net;transport=udp"),
        (
            "sip:bob@example.net",
            "sip:bob@example.net:6000;transport=tcp",
        ),
        (
            "sip:carol@example.org",
            "sip:carol@example.org?Subject=next%20meeting",
        ),
        ("sip:bob@phone21.example.net", "sip:bob@198.51.100.4"),
    ] {
        check::<SipUri>(a, b, false);
    }
}

#[test]
fn sip_params_compared_when_only_one_side_has_them() {
    for param in ["user=phone", "ttl=1", "method=INVITE", "maddr=198.51.100.1"] {
        check::<SipUri>(
            "sip:+15551234567@example.com",
            &format!("sip:+15551234567@example.com;{param}"),
            false,
        );
    }
    check::<SipUri>("sip:example.com;lr;x=1", "sip:example.com;x=2;lr", false);
    check::<SipUri>("sips:alice@example.com", "sip:alice@example.com", false);
    check::<SipUri>("sip:alice:pw@example.com", "sip:alice@example.com", false);
    check::<SipUri>(
        "sip:alice;cpc=x@example.com",
        "sip:alice;CPC=x@example.com",
        false,
    );
    check::<SipUri>("sip:example.com?h=A&h=b", "sip:example.com?H=A&h=b", true);
    check::<SipUri>("sip:example.com?h=A&h=b", "sip:example.com?H=b&h=A", false);
    check::<SipUri>("sip:example.com?h=A", "sip:example.com?h=a", false);
}

#[test]
fn tel_rfc3966_comparison() {
    for (a, b, expected) in [
        ("tel:+1-555-123-4567", "tel:+1.555.(123)4567", true),
        (
            "tel:+15551234567",
            "tel:15551234567;phone-context=+1",
            false,
        ),
        (
            "tel:7042;phone-context=EXAMPLE.com",
            "tel:7042;phone-context=example.COM",
            true,
        ),
        (
            "tel:7042;phone-context=example.com",
            "tel:7042;phone-context=exam-ple.com",
            false,
        ),
        (
            "tel:7042;phone-context=+1-555",
            "tel:7042;phone-context=+1555",
            true,
        ),
        (
            "tel:+15551234567;ext=1-23;isub=AB",
            "tel:+15551234567;isub=ab;ext=123",
            true,
        ),
        ("tel:+15551234567", "tel:+15551234567;cpc=ordinary", false),
        (
            "tel:abc*1;phone-context=example.com",
            "tel:ABC*1;phone-context=example.com",
            true,
        ),
    ] {
        check::<TelUri>(a, b, expected);
    }
}

#[test]
fn urn_ignores_rqf_and_nid_case() {
    check::<UrnUri>("urn:SERVICE:sos", "urn:service:sos?=x#f", true);
    check::<UrnUri>("urn:service:sos", "urn:service:SOS", false);
    check::<UrnUri>("urn:example:a%2fb", "urn:example:a%2Fb", true);
}

#[test]
fn uri_compares_within_one_variant() {
    check::<Uri>(
        "sip:alice@example.com;transport=TCP",
        "sip:alice@example.com;transport=tcp",
        true,
    );
    check::<Uri>("tel:+1-555-123-4567", "tel:+15551234567", true);
    check::<Uri>("urn:service:sos", "URN:Service:sos", true);
    check::<Uri>("https://example.com", "HTTPS://example.com", true);
    check::<Uri>("sip:+15551234567@example.com", "tel:+15551234567", false);
}

/// RFC 3261 §7.3.1: rows with the same name keep their order, other rows
/// may move.
#[test]
fn same_name_headers_compare_in_order() {
    check::<SipUri>(
        "sip:example.com?Route=a&Subject=x&Route=b",
        "sip:example.com?subject=x&route=a&route=b",
        true,
    );
    check::<SipUri>(
        "sip:example.com?Route=a&Route=b",
        "sip:example.com?Route=b&Route=a",
        false,
    );
    check::<SipUri>("sip:example.com?A=1&A=1", "sip:example.com?A=1", false);
}

/// Folds the compact name `s` into `subject` and compares values ignoring
/// case: a stand-in for a header parser's field rules.
struct SubjectRules;

impl UriHeaderEquivalence for SubjectRules {
    fn headers_equivalent(&self, a: &Headers, b: &Headers) -> bool {
        let folded = |h: &Headers| {
            h.iter()
                .map(|(n, v)| {
                    let name = if n.eq_ignore_ascii_case("s") {
                        "subject".to_string()
                    } else {
                        n.to_ascii_lowercase()
                    };
                    (name, v.map(str::to_ascii_lowercase))
                })
                .collect::<Vec<_>>()
        };
        folded(a) == folded(b)
    }
}

#[test]
fn equivalent_with_takes_the_header_rules() {
    let a = SipUri::parse("sip:example.com?s=Lunch").unwrap();
    let b = SipUri::parse("sip:example.com?subject=lunch").unwrap();
    assert!(!a.equivalent(&b));
    assert!(a.equivalent_with(&b, &SubjectRules));
    let rules: &dyn UriHeaderEquivalence = &SubjectRules;
    assert!(a.equivalent_with(&b, rules));
    let (ua, ub) = (Uri::Sip(a), Uri::Sip(b));
    assert!(!ua.equivalent(&ub));
    assert!(ua.equivalent_with(&ub, rules));
    let tel = TelUri::parse("tel:+15551234567").unwrap();
    assert!(tel.equivalent_with(&tel, rules));
}
