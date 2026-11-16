use std::fmt::Debug;

use sip_uri::encoding::decode_user;
use sip_uri::{
    Host, Hostname, OtherUri, SipUri, SipUriParts, TelUri, TelUriParts, Uri, UriParse, UrnUri,
    UrnUriParts, UserParams,
};

fn host() -> Host {
    Host::Hostname("example.com".into())
}

fn hostname(uri: &SipUri) -> Option<String> {
    match uri.host()? {
        Host::Hostname(name) => Some(name.to_string()),
        _ => None,
    }
}

fn only<'a>(
    pairs: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
) -> Option<(String, Option<String>)> {
    let mut pairs = pairs.into_iter();
    match (pairs.next(), pairs.next()) {
        (Some((name, value)), None) => Some((name.to_string(), value.map(str::to_string))),
        _ => None,
    }
}

/// Every byte the parser reads into the component without a warning stays
/// literal in the value built from the same text.
fn sweep<T: UriParse>(
    component: &str,
    uri: impl Fn(&str) -> String,
    parsed: impl Fn(&T) -> Option<String>,
    built: impl Fn(&str) -> Option<String>,
    fold: fn(&str) -> String,
) {
    let mut kept = 0;
    for b in 0u8..0x80 {
        let text = format!("a{}b", b as char);
        let want = fold(&text);
        let Ok(p) = T::parse_with_warnings(&uri(&text)) else {
            continue;
        };
        let Some(got) = parsed(&p.value) else {
            continue;
        };
        if p.has_warnings() || decode_user(&got).as_ref() != want.as_bytes() {
            continue;
        }
        assert_eq!(got, want, "{component}: parsed {:?}", b as char);
        assert_eq!(
            built(&text),
            Some(want),
            "{component}: built {:?}",
            b as char
        );
        kept += 1;
    }
    assert!(kept >= 20, "{component}: only {kept} conformant bytes");
}

fn same(s: &str) -> String {
    s.to_string()
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

#[test]
fn conformant_sip_bytes_are_never_escaped() {
    let base = || SipUri::new(host());
    sweep::<SipUri>(
        "user",
        |t| format!("sip:{t}@example.com"),
        |u| {
            u.user()
                .map(str::to_string)
        },
        |t| {
            base()
                .with_user(t)
                .user()
                .map(str::to_string)
        },
        same,
    );
    sweep::<SipUri>(
        "user-param name",
        |t| format!("sip:u;{t}=v@example.com"),
        |u| only(u.user_params()).map(|(n, _)| n),
        |t| {
            only(
                base()
                    .with_user("u")
                    .with_user_param(t, Some("v"))
                    .user_params(),
            )
            .map(|(n, _)| n)
        },
        same,
    );
    sweep::<SipUri>(
        "user-param value",
        |t| format!("sip:u;n={t}@example.com"),
        |u| only(u.user_params()).and_then(|(_, v)| v),
        |t| {
            only(
                base()
                    .with_user("u")
                    .with_user_param("n", Some(t))
                    .user_params(),
            )
            .and_then(|(_, v)| v)
        },
        same,
    );
    sweep::<SipUri>(
        "password",
        |t| format!("sip:u:{t}@example.com"),
        |u| {
            u.password()
                .map(str::to_string)
        },
        |t| {
            base()
                .with_password(t)
                .password()
                .map(str::to_string)
        },
        same,
    );
    sweep::<SipUri>(
        "hostname",
        |t| format!("sip:{t}"),
        hostname,
        |t| hostname(&SipUri::new(Host::Hostname(t.into()))),
        lower,
    );
    sweep::<SipUri>(
        "param name",
        |t| format!("sip:example.com;{t}=v"),
        |u| only(u.params()).map(|(n, _)| n),
        |t| {
            only(
                base()
                    .with_param(t, Some("v"))
                    .params(),
            )
            .map(|(n, _)| n)
        },
        same,
    );
    sweep::<SipUri>(
        "param value",
        |t| format!("sip:example.com;n={t}"),
        |u| only(u.params()).and_then(|(_, v)| v),
        |t| {
            only(
                base()
                    .with_param("n", Some(t))
                    .params(),
            )
            .and_then(|(_, v)| v)
        },
        same,
    );
    sweep::<SipUri>(
        "header name",
        |t| format!("sip:example.com?{t}=v"),
        |u| only(u.headers()).map(|(n, _)| n),
        |t| {
            only(
                base()
                    .with_header(t, Some("v"))
                    .headers(),
            )
            .map(|(n, _)| n)
        },
        same,
    );
    sweep::<SipUri>(
        "header value",
        |t| format!("sip:example.com?n={t}"),
        |u| only(u.headers()).and_then(|(_, v)| v),
        |t| {
            only(
                base()
                    .with_header("n", Some(t))
                    .headers(),
            )
            .and_then(|(_, v)| v)
        },
        same,
    );
}

#[test]
fn conformant_tel_bytes_are_never_escaped() {
    let base = || TelUri::new("+15551234567");
    sweep::<TelUri>(
        "number",
        |t| format!("tel:{t};phone-context=example.com"),
        |u| {
            u.number()
                .map(str::to_string)
        },
        |t| {
            TelUri::new(t)
                .number()
                .map(str::to_string)
        },
        same,
    );
    sweep::<TelUri>(
        "param name",
        |t| format!("tel:+15551234567;{t}=v"),
        |u| only(u.params()).map(|(n, _)| n),
        |t| {
            only(
                base()
                    .with_param(t, Some("v"))
                    .params(),
            )
            .map(|(n, _)| n)
        },
        same,
    );
    sweep::<TelUri>(
        "param value",
        |t| format!("tel:+15551234567;n={t}"),
        |u| only(u.params()).and_then(|(_, v)| v),
        |t| {
            only(
                base()
                    .with_param("n", Some(t))
                    .params(),
            )
            .and_then(|(_, v)| v)
        },
        same,
    );
}

#[test]
fn conformant_urn_bytes_are_never_escaped() {
    sweep::<UrnUri>(
        "nid",
        |t| format!("urn:{t}:x"),
        |u| {
            u.nid()
                .map(str::to_string)
        },
        |t| {
            UrnUri::new(t, "x")
                .nid()
                .map(str::to_string)
        },
        lower,
    );
    sweep::<UrnUri>(
        "nss",
        |t| format!("urn:example:{t}"),
        |u| {
            u.nss()
                .map(str::to_string)
        },
        |t| {
            UrnUri::new("example", t)
                .nss()
                .map(str::to_string)
        },
        same,
    );
    sweep::<UrnUri>(
        "r-component",
        |t| format!("urn:example:x?+{t}"),
        |u| {
            u.r_component()
                .map(str::to_string)
        },
        |t| {
            UrnUri::new("example", "x")
                .with_r_component(t)
                .r_component()
                .map(str::to_string)
        },
        same,
    );
    sweep::<UrnUri>(
        "q-component",
        |t| format!("urn:example:x?={t}"),
        |u| {
            u.q_component()
                .map(str::to_string)
        },
        |t| {
            UrnUri::new("example", "x")
                .with_q_component(t)
                .q_component()
                .map(str::to_string)
        },
        same,
    );
    sweep::<UrnUri>(
        "f-component",
        |t| format!("urn:example:x#{t}"),
        |u| {
            u.f_component()
                .map(str::to_string)
        },
        |t| {
            UrnUri::new("example", "x")
                .with_f_component(t)
                .f_component()
                .map(str::to_string)
        },
        same,
    );
}

/// Component text exercising every delimiter, escape shape and a non-ASCII byte.
fn samples() -> Vec<String> {
    let mut out: Vec<String> = (0u8..0x80)
        .map(|b| format!("a{}b", b as char))
        .collect();
    for s in [
        "",
        "a",
        "é",
        "%41",
        "%3b",
        "%2B",
        "%zz",
        "%",
        "%2",
        "#",
        "?+",
        "?=",
        ":",
        "@",
        ";",
        "5060",
        "a:5060",
        "198.51.100.1",
        "sip",
        "sips",
        "tel",
        "urn",
        "x-test",
        "\r\n",
    ] {
        out.push(s.to_string());
    }
    out
}

#[derive(Default)]
struct Failures(Vec<String>);

impl Failures {
    fn round_trips<T: UriParse + ToString + PartialEq + Debug>(&mut self, value: &T) {
        let text = value.to_string();
        match T::parse(&text) {
            Ok(reparsed) if &reparsed == value => {}
            Ok(reparsed) => self
                .0
                .push(format!("{text:?}: {value:?} became {reparsed:?}")),
            Err(e) => self
                .0
                .push(format!("{text:?}: {e}")),
        }
    }

    fn round_trips_as_uri(&mut self, value: impl Into<Uri>) {
        self.round_trips(&value.into());
    }

    fn assert_none(self) {
        assert!(
            self.0
                .is_empty(),
            "{} failures:\n{}",
            self.0
                .len(),
            self.0
                .join("\n")
        );
    }
}

fn sip_values(s: &str) -> Vec<SipUri> {
    let base = || SipUri::new(host());
    vec![
        base().with_user(s),
        base()
            .with_user("u")
            .with_user_param(s, None),
        base()
            .with_user("u")
            .with_user_param(s, Some(s)),
        base()
            .with_user("u")
            .with_user_param(s, Some("v")),
        base()
            .with_user("u")
            .with_user_param("n", Some(s)),
        base().with_user_param(s, Some("v")),
        base()
            .with_user(s)
            .with_user_param("", Some(s)),
        base()
            .with_user_param("", Some(s))
            .with_password(s),
        base()
            .with_user_params(
                UserParams::new()
                    .with("", Some(s))
                    .with(s, None),
            )
            .with_param("", Some(s)),
        base().with_password(s),
        base()
            .with_user("u")
            .with_password(s),
        base().with_param(s, None),
        base().with_param(s, Some(s)),
        base().with_param(s, Some("v")),
        base().with_param("n", Some(s)),
        base().with_header(s, Some(s)),
        base().with_header("n", Some(s)),
        base().with_header(s, None),
        base()
            .with_header("h", None)
            .with_header(s, None),
        base().with_fragment(s),
        base()
            .with_param("n", None)
            .with_header("h", Some("v"))
            .with_fragment(s),
        SipUri::new(Host::Hostname(Hostname::from(s))),
        SipUri::new(Host::Hostname(Hostname::from(s)))
            .with_user("u")
            .with_port(5060),
        base()
            .with_user("u")
            .with_host(Host::Hostname(Hostname::from(s))),
        edit(base().with_user_param(s, Some("v")), |u| {
            u.user_params_mut()
                .set(s, Some(s));
        }),
        edit(base(), |u| {
            u.user_params_mut()
                .set("n", Some(s));
        }),
        edit(base().with_user_param(s, None), |u| {
            u.user_params_mut()
                .retain(|_, _| false);
        }),
        edit(
            base()
                .with_param(s, None)
                .with_param("n", None),
            |u| {
                u.params_mut()
                    .set("n", Some(s));
                u.params_mut()
                    .remove(s);
            },
        ),
        edit(base().with_header(s, None), |u| {
            u.headers_mut()
                .set(s, Some(s));
        }),
    ]
}

fn edit<T>(mut value: T, f: impl FnOnce(&mut T)) -> T {
    f(&mut value);
    value
}

#[test]
fn edits_hold_what_builders_hold() {
    let base = || SipUri::new(host()).with_user("u");
    assert_eq!(
        edit(SipUri::new(host()), |u| {
            u.user_params_mut()
                .push("cpc", Some("a b"));
        }),
        SipUri::new(host()).with_user_param("cpc", Some("a b"))
    );
    assert_eq!(
        edit(SipUri::new(host()).with_user_param("cpc", None), |u| {
            u.user_params_mut()
                .remove("CPC");
        }),
        SipUri::new(host())
    );
    assert_eq!(
        edit(base().with_param("transport", Some("udp")), |u| {
            u.params_mut()
                .set("transport", Some("t;x"));
            u.headers_mut()
                .set("h", None);
        }),
        base()
            .with_param("transport", Some("t;x"))
            .with_header("h", None)
    );
    assert_eq!(
        base().with_host(Host::Hostname("198.51.100.1".into())),
        SipUri::parse("sip:u@198.51.100.1").unwrap()
    );
    assert_eq!(
        base()
            .with_host(Host::Hostname("".into()))
            .host(),
        None
    );
    assert_eq!(
        edit(TelUri::new("+15551234567"), |t| {
            t.params_mut()
                .set("cpc", Some("x"));
        }),
        TelUri::new("+15551234567").with_param("cpc", Some("x"))
    );
}

fn tel_values(s: &str) -> Vec<TelUri> {
    let base = || TelUri::new("+15551234567");
    // A fragment prints as tel: number digits unless a param precedes it.
    vec![
        TelUri::new(s),
        TelUri::new(s).with_param("n", None),
        base().with_param(s, None),
        base().with_param(s, Some(s)),
        base().with_param("n", Some(s)),
        base()
            .with_param("n", None)
            .with_fragment(s),
        TelUri::from(TelUriParts::default()).with_param("n", Some(s)),
        edit(base().with_param(s, None), |t| {
            t.params_mut()
                .set(s, Some(s));
        }),
    ]
}

fn urn_values(s: &str) -> Vec<UrnUri> {
    let parts = |nid: Option<&str>, nss: Option<&str>| {
        let mut p = UrnUriParts::default();
        p.nid = nid.map(str::to_string);
        p.nss = nss.map(str::to_string);
        p
    };
    let mut out = vec![
        UrnUri::new(s, "x"),
        UrnUri::new("example", s),
        UrnUri::new("example", "x").with_r_component(s),
        UrnUri::new("example", "x").with_q_component(s),
        UrnUri::new("example", "x").with_f_component(s),
        UrnUri::new("example", "x")
            .with_r_component(s)
            .with_q_component(s)
            .with_f_component(s),
    ];
    for (nid, nss) in [(Some("example"), None), (None, Some("x")), (None, None)] {
        out.push(UrnUri::from(parts(nid, nss)).with_r_component(s));
        out.push(UrnUri::from(parts(nid, nss)).with_q_component(s));
        out.push(UrnUri::from(parts(nid, nss)).with_f_component(s));
    }
    out
}

#[test]
fn constructed_values_survive_the_parser() {
    let mut failures = Failures::default();
    for s in samples() {
        for v in sip_values(&s) {
            failures.round_trips(&v);
            failures.round_trips_as_uri(v);
        }
        for v in tel_values(&s) {
            failures.round_trips(&v);
            failures.round_trips_as_uri(v);
        }
        for v in urn_values(&s) {
            failures.round_trips(&v);
            failures.round_trips_as_uri(v);
        }
        match OtherUri::new(Some("x-test"), &s) {
            Ok(other) if !reads_as_port(&s) => failures.round_trips_as_uri(Uri::Other(other)),
            _ => {}
        }
    }
    failures.assert_none();
}

/// A schemed `Other` whose rest starts with digits alone re-parses as a
/// scheme-less `host:port`.
fn reads_as_port(rest: &str) -> bool {
    let port = rest
        .split([';', '?', '#', '/', '>'])
        .next()
        .unwrap_or_default();
    !port.is_empty()
        && port
            .bytes()
            .all(|b| b.is_ascii_digit())
}

fn survives<T: UriParse + ToString + PartialEq>(value: &T) -> bool {
    T::parse(&value.to_string()).is_ok_and(|reparsed| &reparsed == value)
}

/// The text begins like a scheme: the parser reads one from it.
fn reads_as_scheme(text: &str) -> bool {
    Uri::parse(text).is_ok_and(|uri| {
        uri.scheme()
            .is_some()
    })
}

fn schemeless_sip_values(s: &str) -> Vec<SipUri> {
    let parts = |user: Option<&str>, password: Option<&str>, host: &str, port: Option<u16>| {
        let mut p = SipUriParts::default();
        p.user = user.map(str::to_string);
        p.password = password.map(str::to_string);
        p.host = Some(Host::Hostname(host.into()));
        p.port = port;
        SipUri::from(p)
    };
    let mut out = vec![
        parts(Some(s), None, "example.com", None),
        parts(Some(s), Some("pw"), "example.com", None),
        parts(Some(s), Some("5060"), "example.com", None),
        parts(Some("u"), Some(s), "example.com", None),
        parts(None, Some(s), "example.com", None),
        parts(Some(s), None, "example.com", Some(5060)),
        parts(None, None, s, Some(5060)),
    ];
    for scheme in ["sip", "sips", "tel", "urn"] {
        out.push(parts(None, None, scheme, Some(5060)));
        out.push(parts(Some(scheme), Some(s), "example.com", None));
    }
    out
}

#[test]
fn schemeless_sip_uri_fails_only_where_its_text_reads_as_a_scheme() {
    let mut failures = Vec::new();
    let mut exceptions = 0;
    for s in samples() {
        for uri in schemeless_sip_values(&s) {
            let text = uri.to_string();
            // Its `:` ends a scheme, readable or not, before it can start a password.
            let exception = reads_as_scheme(&text)
                || uri
                    .password()
                    .is_some();
            if survives(&uri) == exception {
                failures.push(format!("{text:?}: {uri:?}"));
            }
            exceptions += usize::from(exception);
            if survives(&Uri::Sip(uri)) {
                failures.push(format!("{text:?}: Uri reads it as a SIP URI"));
            }
        }
    }
    assert!(exceptions > 0);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn schemeless_other_fails_only_where_its_text_reads_as_a_scheme() {
    let mut failures = Vec::new();
    for s in samples() {
        if let Ok(other) = OtherUri::new(None, &s) {
            let uri = Uri::Other(other);
            if survives(&uri) == reads_as_scheme(&uri.to_string()) {
                failures.push(format!("{uri:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn standalone_host_survives_unless_empty_or_dotted_quad() {
    for s in samples() {
        let host = Host::Hostname(
            s.as_str()
                .into(),
        );
        let reparsed = Host::parse(&host.to_string());
        match Host::from_hostname(s.as_str()) {
            Host::Hostname(name)
                if name
                    .as_str()
                    .is_empty() =>
            {
                assert!(reparsed.is_err(), "{s:?}")
            }
            Host::IPv4(addr) => assert_eq!(reparsed.unwrap(), Host::IPv4(addr), "{s:?}"),
            _ => assert_eq!(reparsed.unwrap(), host, "{s:?}"),
        }
    }
}

/// Every strict-valid input with one escape in `template`'s `{}` displays as
/// strict-valid text.
fn strict_display<T: UriParse + ToString>(template: &str) {
    let mut checked = 0;
    for b in 0u8..=255 {
        for escape in [format!("%{b:02X}"), format!("%{b:02x}")] {
            let input = template.replace("{}", &escape);
            let Ok(value) = T::parse_strict(&input) else {
                continue;
            };
            let text = value.to_string();
            assert!(
                T::parse_strict(&text).is_ok(),
                "{input:?} displays as {text:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "{template}");
}

#[test]
fn strict_input_displays_as_strict_input() {
    for template in [
        "sip:a{}b@example.com",
        "sip:u;a{}b=v@example.com",
        "sip:u;n=a{}b@example.com",
        "sip:u:a{}b@example.com",
        "sip:example.com;a{}b=v",
        "sip:example.com;n=a{}b",
        "sip:example.com?a{}b=v",
        "sip:example.com?n=a{}b",
    ] {
        strict_display::<SipUri>(template);
    }
    strict_display::<TelUri>("tel:+15551234567;n=a{}b");
    for template in [
        "urn:example:a{}b",
        "urn:example:x?+a{}b",
        "urn:example:x?=a{}b",
        "urn:example:x#a{}b",
    ] {
        strict_display::<UrnUri>(template);
    }
}

#[test]
fn a_param_holds_a_literal_comma_or_at_escaped() {
    let uri = SipUri::parse("sip:u@example.com;x=a,b@c").unwrap();
    assert_eq!(uri.to_string(), "sip:u@example.com;x=a%2Cb%40c");
    assert!(SipUri::parse_strict(&uri.to_string()).is_ok());
}

#[test]
fn escaped_reserved_characters_stay_escaped() {
    let parse = |s: &str| SipUri::parse(s).unwrap();
    assert_ne!(
        parse("sip:%2B15551234567@example.com"),
        parse("sip:+15551234567@example.com")
    );
    assert_eq!(
        parse("sip:%61lice@example.com"),
        parse("sip:alice@example.com")
    );
    assert_eq!(
        parse("sip:example.com;x=a%2fb").to_string(),
        "sip:example.com;x=a%2Fb"
    );
    assert_eq!(
        parse("sip:a%3Fb@example.com").to_string(),
        "sip:a%3Fb@example.com"
    );
}

#[test]
fn named_exceptions_take_the_other_reading() {
    let other = Uri::Other(OtherUri::new(None, "x-test:y").unwrap());
    assert_eq!(
        Uri::parse(&other.to_string())
            .unwrap()
            .scheme(),
        Some("x-test")
    );

    let port = Uri::Other(OtherUri::new(Some("x-test"), "5060").unwrap());
    assert_eq!(
        Uri::parse(&port.to_string())
            .unwrap()
            .scheme(),
        None
    );

    let tel = TelUri::new("+15551234567").with_fragment("x");
    assert_eq!(
        TelUri::parse(&tel.to_string())
            .unwrap()
            .number(),
        Some("+15551234567#x")
    );
}

#[test]
fn no_cr_lf_or_nul_survives_display() {
    for c in ["\r", "\n", "\0"] {
        let t = |s: &str| s.replace('$', c);
        let mut shown: Vec<String> = [
            "sip:u$;up=$:p$@h$.example.com;x=$?h=$#$",
            "$sip:u@example.com",
            "tel:+1555$;x=$#$",
            "urn:ex$:a$?+$?=$#$",
            "x-test:a$",
            "$",
        ]
        .iter()
        .filter_map(|input| Uri::parse(&t(input)).ok())
        .map(|uri| uri.to_string())
        .collect();
        shown.push(
            SipUri::new(Host::Hostname(t("h$").into()))
                .with_user(t("u$"))
                .with_user_param(t("n$"), Some(&t("v$")))
                .with_password(t("p$"))
                .with_param(t("n$"), Some(&t("v$")))
                .with_header(t("n$"), Some(&t("v$")))
                .with_fragment(t("f$"))
                .to_string(),
        );
        shown.push(
            TelUri::new(t("+1555$"))
                .with_param(t("n$"), Some(&t("v$")))
                .with_fragment(t("f$"))
                .to_string(),
        );
        shown.push(
            UrnUri::new(t("ex$"), t("a$"))
                .with_r_component(t("r$"))
                .with_q_component(t("q$"))
                .with_f_component(t("f$"))
                .to_string(),
        );
        if let Ok(other) = OtherUri::new(Some("x-test"), &t("a$")) {
            shown.push(other.to_string());
        }
        for text in shown {
            assert!(!text.contains(c), "{c:?} survived in {text:?}");
        }
    }
}
