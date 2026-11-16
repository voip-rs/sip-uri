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
/// each pair as written to `push` for the data crate to canonize.
pub(crate) fn parse_params<'a>(
    s: &'a str,
    grammar: &ParamGrammar,
    warnings: &mut Warnings,
    mut push: impl FnMut(&'a str, Option<&'a str>),
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
        push(name, value);
    }
}

/// Split header parameters following a `?`: `name=value` pairs separated by
/// `&`, each handed as written to `push`.
pub(crate) fn parse_headers<'a>(
    s: &'a str,
    warnings: &mut Warnings,
    mut push: impl FnMut(&'a str, Option<&'a str>),
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
        push(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sip_params(s: &str) -> Vec<(&str, Option<&str>)> {
        let mut w = Warnings::new(s);
        let mut params = Vec::new();
        parse_params(s, &SIP_PARAMS, &mut w, |n, v| params.push((n, v)));
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
    fn parse_headers_basic() {
        let s = "From=foo@bar&To=bar@baz&Flag";
        let mut headers = Vec::new();
        parse_headers(s, &mut Warnings::new(s), |n, v| headers.push((n, v)));
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
