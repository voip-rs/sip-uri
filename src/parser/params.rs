use crate::grammar::{CharClass, HNV, PARAM, TEL_PARAMCHAR, TEL_PNAME, USER};
use crate::warning::{Component, WarningCode, Warnings};

/// Character sets for one kind of `;`-separated param list.
pub(crate) struct ParamGrammar {
    component: Component,
    /// Conformant names and values; any other byte warns.
    name: CharClass,
    value: CharClass,
}

/// SIP URI params after the host.
pub(crate) const SIP_PARAMS: ParamGrammar = ParamGrammar {
    component: Component::Param,
    name: PARAM,
    value: PARAM,
};

/// SIP user-params inside the userinfo.
pub(crate) const USER_PARAMS: ParamGrammar = ParamGrammar {
    component: Component::UserParam,
    name: USER,
    value: USER,
};

/// tel: URI params (RFC 3966).
pub(crate) const TEL_PARAMS: ParamGrammar = ParamGrammar {
    component: Component::Param,
    name: TEL_PNAME,
    value: TEL_PARAMCHAR,
};

/// Split the params following a `;`, which is not included in `s`, handing
/// each pair as written to `push` for the data crate to canonize. `push`
/// returns whether the list held the pair; one it could not is reported.
pub(crate) fn parse_params<'a>(
    s: &'a str,
    grammar: &ParamGrammar,
    warnings: &mut Warnings,
    mut push: impl FnMut(&'a str, Option<&'a str>) -> bool,
) {
    let component = grammar.component;
    for part in s.split(';') {
        if part.is_empty() {
            warnings.push(component, WarningCode::EmptySegment, part, 0);
            continue;
        }
        let (name, value) = match part.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (part, None),
        };
        if name.is_empty() {
            warnings.push(component, WarningCode::EmptyName, part, 0);
        }
        warnings.charset(component, name, grammar.name);
        if let Some(value) = value {
            warnings.charset(component, value, grammar.value);
        }
        if !push(name, value) {
            warnings.push(component, WarningCode::TooLong, part, 0);
        }
    }
}

/// Split header parameters following a `?`: `name=value` pairs separated by
/// `&`, each handed as written to `push` as [`parse_params`] does.
pub(crate) fn parse_headers<'a>(
    s: &'a str,
    warnings: &mut Warnings,
    mut push: impl FnMut(&'a str, Option<&'a str>) -> bool,
) {
    for part in s.split('&') {
        if part.is_empty() {
            warnings.push(Component::Header, WarningCode::EmptySegment, part, 0);
            continue;
        }
        let (name, value) = match part.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => {
                warnings.push(
                    Component::Header,
                    WarningCode::MissingValue,
                    part,
                    part.len(),
                );
                (part, None)
            }
        };
        if name.is_empty() {
            warnings.push(Component::Header, WarningCode::EmptyName, part, 0);
        }
        warnings.charset(Component::Header, name, HNV);
        if let Some(value) = value {
            warnings.charset(Component::Header, value, HNV);
        }
        if !push(name, value) {
            warnings.push(Component::Header, WarningCode::TooLong, part, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sip_params(s: &str) -> Vec<(&str, Option<&str>)> {
        let mut w = Warnings::new(s);
        let mut params = Vec::new();
        parse_params(s, &SIP_PARAMS, &mut w, |n, v| {
            params.push((n, v));
            true
        });
        assert!(!w
            .finish(())
            .has_warnings());
        params
    }

    #[test]
    fn parse_key_value() {
        assert_eq!(sip_params("transport=tcp"), [("transport", Some("tcp"))]);
    }

    #[test]
    fn parse_mixed() {
        assert_eq!(
            sip_params("user=phone;ttl=1;isfocus"),
            [
                ("user", Some("phone")),
                ("ttl", Some("1")),
                ("isfocus", None)
            ]
        );
    }

    #[test]
    fn a_pair_the_list_cannot_hold_is_reported_dropped() {
        use crate::warning::WarningKind;
        let refuse = |name: &str| name.starts_with('b');
        let s = "a=1;b=2;c";
        let mut w = Warnings::new(s);
        parse_params(s, &USER_PARAMS, &mut w, |n, _| !refuse(n));
        let s2 = "a=1&bb";
        let mut w2 = Warnings::new(s2);
        parse_headers(s2, &mut w2, |n, _| !refuse(n));
        let found: Vec<_> = [w.finish(()), w2.finish(())]
            .into_iter()
            .flat_map(|p| p.warnings)
            .filter(|w| w.code == WarningCode::TooLong)
            .map(|w| (w.component, w.position, w.kind))
            .collect();
        assert_eq!(
            found,
            [
                (Component::UserParam, Some(4), WarningKind::Lost),
                (Component::Header, Some(4), WarningKind::Lost)
            ]
        );
    }

    #[test]
    fn parse_headers_basic() {
        let s = "From=foo@bar&To=bar@baz&Flag";
        let mut headers = Vec::new();
        parse_headers(s, &mut Warnings::new(s), |n, v| {
            headers.push((n, v));
            true
        });
        assert_eq!(
            headers,
            [
                ("From", Some("foo@bar")),
                ("To", Some("bar@baz")),
                ("Flag", None)
            ]
        );
    }
}
