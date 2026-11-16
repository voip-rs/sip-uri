//! Items that moved to sip-uri-diagnostics and sip-uri-types keep their
//! `sip_uri::` paths, and each path names the very item the owning crate
//! exports, so a value passes between the two spellings unconverted.

#[test]
fn diagnostics_resolve_through_both_crates() {
    use sip_uri_diagnostics as d;

    let component: fn(sip_uri::Component) -> d::Component = |x| x;
    let code: fn(sip_uri::WarningCode) -> d::WarningCode = |x| x;
    let kind: fn(sip_uri::WarningKind) -> d::WarningKind = |x| x;
    let warning: fn(sip_uri::ParseWarning) -> d::ParseWarning = |x| x;
    let error: fn(d::ParseError) -> sip_uri::ParseError = |x| x;
    let crate_path: fn(sip_uri::sip_uri_diagnostics::ParseError) -> d::ParseError = |x| x;

    let w = warning(d::ParseWarning::new(
        component(sip_uri::Component::Port),
        code(sip_uri::WarningCode::SignedPort),
        Some(16),
    ));
    assert_eq!(kind(w.kind), sip_uri::WarningKind::Recovered);
    assert_eq!(
        crate_path(error(d::ParseError::NonConformant(w))),
        sip_uri::ParseError::NonConformant(w)
    );
}
