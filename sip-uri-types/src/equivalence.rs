//! How two [`Headers`] sets compare: the trait a SIP header parser
//! implements with each field's own rules, and the default rule without
//! them, RFC 3261 §7.3.1's row order with exact values.
//!
//! They live beside [`Headers`] because a header parser implements the trait
//! against it and names it in its own API, which would otherwise depend on
//! sip-uri's 0.x releases. The RFC equivalence rules that use them, and the
//! choice of which header rule a comparison applies, live in sip-uri.

use std::rc::Rc;
use std::sync::Arc;

use crate::Headers;

/// The header-field matching rules sip-uri's `UriEquivalence::equivalent_with`
/// applies to two SIP URIs' header sets.
///
/// RFC 3261 §19.1.4 matches each header by its own field's rules, which a
/// SIP header parser supplies; [`ordered_headers_equivalent`] is the rule
/// without them, to fall back on for a field the implementor does not know.
///
/// ```
/// use sip_uri_types::{Headers, UriHeaderEquivalence};
///
/// struct CaseFolded;
///
/// impl UriHeaderEquivalence for CaseFolded {
///     fn headers_equivalent(&self, a: &Headers, b: &Headers) -> bool {
///         a.len() == b.len()
///             && a.iter().zip(b.iter()).all(|((an, av), (bn, bv))| {
///                 an.eq_ignore_ascii_case(bn)
///                     && av.map(str::to_ascii_lowercase) == bv.map(str::to_ascii_lowercase)
///             })
///     }
/// }
///
/// let a = Headers::new().with("subject", Some("Lunch"));
/// let b = Headers::new().with("subject", Some("lunch"));
/// assert!(!sip_uri_types::ordered_headers_equivalent(&a, &b));
/// assert!(CaseFolded.headers_equivalent(&a, &b));
/// ```
///
/// The trait is object-safe and stays so, and references and smart
/// pointers to an implementor implement it, so a `&dyn`, `Box<dyn>` or
/// `Arc` held by the caller can be passed as it is.
pub trait UriHeaderEquivalence {
    /// Whether the header sets `a` and `b` match. Names and values are the
    /// canonical, percent-encoded text the URI holds.
    fn headers_equivalent(&self, a: &Headers, b: &Headers) -> bool;
}

macro_rules! forward_header_equivalence {
    ($($ptr:ty),*) => {$(
        impl<T: UriHeaderEquivalence + ?Sized> UriHeaderEquivalence for $ptr {
            fn headers_equivalent(&self, a: &Headers, b: &Headers) -> bool {
                (**self).headers_equivalent(a, b)
            }
        }
    )*};
}

forward_header_equivalence!(&T, &mut T, Box<T>, Rc<T>, Arc<T>);

/// URI headers equivalent by RFC 3261 §7.3.1 row order with exact values:
/// rows with different names in any order, rows sharing a name, compared
/// ASCII-case-insensitively, in order and with byte-equal canonical values.
///
/// This is sip-uri's `UriEquivalence::equivalent` header rule. It knows no
/// header field's own matching rules, which RFC 3261 §19.1.4 asks for and a
/// SIP header parser supplies.
///
/// ```
/// use sip_uri_types::{ordered_headers_equivalent, Headers};
///
/// let a = Headers::new()
///     .with("Route", Some("a"))
///     .with("Subject", Some("x"))
///     .with("Route", Some("b"));
/// let moved = Headers::new()
///     .with("subject", Some("x"))
///     .with("route", Some("a"))
///     .with("route", Some("b"));
/// let swapped = Headers::new()
///     .with("Route", Some("b"))
///     .with("Subject", Some("x"))
///     .with("Route", Some("a"));
/// assert!(ordered_headers_equivalent(&a, &moved));
/// assert!(!ordered_headers_equivalent(&a, &swapped));
/// ```
pub fn ordered_headers_equivalent(a: &Headers, b: &Headers) -> bool {
    fn values_named<'a>(list: &'a Headers, name: &str) -> impl Iterator<Item = Option<&'a str>> {
        let name = name.to_ascii_lowercase();
        list.iter()
            .filter(move |(n, _)| n.eq_ignore_ascii_case(&name))
            .map(|(_, v)| v)
    }
    a.len() == b.len()
        && a.iter()
            .all(|(name, _)| values_named(a, name).eq(values_named(b, name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(rows: &[(&str, Option<&str>)]) -> Headers {
        rows.iter()
            .copied()
            .collect()
    }

    #[test]
    fn same_name_rows_keep_their_order() {
        let a = headers(&[
            ("Route", Some("a")),
            ("Subject", Some("x")),
            ("Route", Some("b")),
        ]);
        let b = headers(&[
            ("subject", Some("x")),
            ("route", Some("a")),
            ("route", Some("b")),
        ]);
        assert!(ordered_headers_equivalent(&a, &b));
        let a = headers(&[("Route", Some("a")), ("Route", Some("b"))]);
        let b = headers(&[("Route", Some("b")), ("Route", Some("a"))]);
        assert!(!ordered_headers_equivalent(&a, &b));
    }

    #[test]
    fn a_repeated_row_is_not_the_row_once() {
        let a = headers(&[("A", Some("1")), ("A", Some("1"))]);
        let b = headers(&[("A", Some("1"))]);
        assert!(!ordered_headers_equivalent(&a, &b));
        assert!(!ordered_headers_equivalent(&b, &a));
    }

    #[test]
    fn values_compare_exactly() {
        let a = headers(&[("Subject", Some("Lunch"))]);
        assert!(!ordered_headers_equivalent(
            &a,
            &headers(&[("Subject", Some("lunch"))])
        ));
        assert!(!ordered_headers_equivalent(
            &a,
            &headers(&[("Subject", None)])
        ));
    }
}
