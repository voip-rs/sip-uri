//! Ordered `(name, value)` lists for params, user-params and headers.
//!
//! Each list canonizes through its own component's canonizer, and so does a
//! name it is asked to look up, before the case-insensitive compare: `get`,
//! `remove` and `set` agree on which pairs a name matches however it is
//! spelled.
//!
//! A lookup compares the name as given first and canonizes it only when
//! nothing matches: a name equal to a held canonical name ignoring ASCII
//! case is itself canonical up to case, so both comparisons find the same
//! pairs, and the common lookup costs no canonization.

use std::borrow::Cow;
use std::fmt;

use crate::canon;

type Pair = (String, Option<String>);

type Canonize = fn(&str, Option<&str>) -> Pair;

/// Iterator over the `(name, value)` pairs of a [`Params`], [`UserParams`] or
/// [`Headers`], in order.
#[derive(Debug, Clone)]
pub struct Pairs<'a>(std::slice::Iter<'a, Pair>);

impl<'a> Iterator for Pairs<'a> {
    type Item = (&'a str, Option<&'a str>);

    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .next()
            .map(|(name, value)| (name.as_str(), value.as_deref()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0
            .size_hint()
    }
}

impl DoubleEndedIterator for Pairs<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0
            .next_back()
            .map(|(name, value)| (name.as_str(), value.as_deref()))
    }
}

impl ExactSizeIterator for Pairs<'_> {}

/// Push the canonized pair, unless it is a valueless empty name: its bare
/// delimiter parses as nothing.
fn push_pair(pairs: &mut Vec<Pair>, name: &str, value: Option<&str>, canonize: Canonize) {
    if !name.is_empty() || value.is_some() {
        pairs.push(canonize(name, value));
    }
}

macro_rules! pair_list {
    ($(#[$doc:meta])* $name:ident, $canonize:path, $canonize_name:path) => {
        $(#[$doc])*
        ///
        /// Names and values are held in canonical form: [`push`](Self::push),
        /// [`set`](Self::set), [`From`] and [`FromIterator`] canonize what
        /// they are given, and a
        /// pair with an empty name and no value is dropped. Order and
        /// duplicates are kept.
        ///
        /// With the `serde` feature it serializes as a sequence of
        /// `[name, value]` pairs, the value `null` when absent, and
        /// deserializes through the same canonization.
        #[derive(Clone, Default, PartialEq, Eq, Hash)]
        pub struct $name(Vec<Pair>);

        impl $name {
            /// An empty list.
            pub fn new() -> Self {
                Self::default()
            }

            /// Number of pairs.
            pub fn len(&self) -> usize {
                self.0
                    .len()
            }

            /// Whether there are no pairs.
            pub fn is_empty(&self) -> bool {
                self.0
                    .is_empty()
            }

            /// The `(name, value)` pairs, in order.
            pub fn iter(&self) -> Pairs<'_> {
                Pairs(
                    self.0
                        .iter(),
                )
            }

            /// The value of the first pair named `name`, canonized and
            /// compared case-insensitively: `Some(None)` when it has no value.
            pub fn get(&self, name: &str) -> Option<Option<&str>> {
                let find = |name: &str| {
                    self.iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(name))
                        .map(|(_, v)| v)
                };
                if self.0.is_empty() {
                    return None;
                }
                find(name).or_else(|| match $canonize_name(name) {
                    Cow::Owned(canonical) => find(&canonical),
                    Cow::Borrowed(_) => None,
                })
            }

            /// Append a pair, canonized.
            pub fn push(&mut self, name: impl AsRef<str>, value: Option<&str>) {
                push_pair(&mut self.0, name.as_ref(), value, $canonize);
            }

            /// Append a pair, canonized, and return the list.
            pub fn with(mut self, name: impl AsRef<str>, value: Option<&str>) -> Self {
                self.push(name, value);
                self
            }

            /// Remove every pair named `name`, canonized and compared
            /// case-insensitively, and return how many were removed.
            pub fn remove(&mut self, name: &str) -> usize {
                let held = self
                    .0
                    .iter()
                    .any(|(n, _)| n.eq_ignore_ascii_case(name));
                let name = if held {
                    Cow::Borrowed(name)
                } else {
                    $canonize_name(name)
                };
                let before = self.0.len();
                self.0
                    .retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
                before - self.0.len()
            }

            /// Keep only the pairs for which `keep` returns `true`, in order.
            pub fn retain(&mut self, mut keep: impl FnMut(&str, Option<&str>) -> bool) {
                self.0
                    .retain(|(n, v)| keep(n, v.as_deref()));
            }

            /// Set the pair named `name`, canonized: the first pair whose
            /// canonical name matches case-insensitively is replaced in place
            /// and every later match removed, so the name is left once; with
            /// no match the pair is appended. An empty name without a value is
            /// dropped, as by [`push`](Self::push).
            pub fn set(&mut self, name: impl AsRef<str>, value: Option<&str>) {
                let name = name.as_ref();
                if name.is_empty() && value.is_none() {
                    return;
                }
                let (name, value) = $canonize(name, value);
                let matches = |n: &String| n.eq_ignore_ascii_case(&name);
                let Some(first) = self.0.iter().position(|(n, _)| matches(n)) else {
                    self.0.push((name, value));
                    return;
                };
                let mut index = 0;
                self.0.retain(|(n, _)| {
                    index += 1;
                    index <= first + 1 || !matches(n)
                });
                if let Some(slot) = self.0.get_mut(first) {
                    *slot = (name, value);
                }
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_list()
                    .entries(self.iter())
                    .finish()
            }
        }

        impl<'a> IntoIterator for &'a $name {
            type Item = (&'a str, Option<&'a str>);
            type IntoIter = Pairs<'a>;

            fn into_iter(self) -> Pairs<'a> {
                self.iter()
            }
        }

        impl<N: AsRef<str>, V: AsRef<str>> FromIterator<(N, Option<V>)> for $name {
            fn from_iter<I: IntoIterator<Item = (N, Option<V>)>>(pairs: I) -> Self {
                let mut list = Self::default();
                for (name, value) in pairs {
                    list.push(
                        name,
                        value
                            .as_ref()
                            .map(AsRef::as_ref),
                    );
                }
                list
            }
        }

        impl From<Vec<(String, Option<String>)>> for $name {
            fn from(pairs: Vec<(String, Option<String>)>) -> Self {
                pairs
                    .into_iter()
                    .collect()
            }
        }
    };
}

pair_list!(
    /// Parameters of a SIP URI after the host, or of a tel: URI.
    Params,
    canon::canonize_param,
    canon::param_name
);

pair_list!(
    /// Parameters inside a SIP URI's userinfo, after the user part.
    UserParams,
    canon::canonize_user_param,
    canon::user_param_name
);

pair_list!(
    /// Headers of a SIP URI, after `?`. A header written without `=` has no
    /// value, and one written `name=` has an empty one.
    Headers,
    canon::canonize_header,
    canon::header_name
);

/// Write each pair as `;name` or `;name=value`.
pub(crate) fn format_params(pairs: Pairs<'_>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (name, value) in pairs {
        write!(f, ";{name}")?;
        if let Some(v) = value {
            write!(f, "={v}")?;
        }
    }
    Ok(())
}

/// Write headers as `?name=value&name`, a header without a value as its name.
pub(crate) fn format_headers(headers: &Headers, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (i, (name, value)) in headers
        .iter()
        .enumerate()
    {
        let sep = if i == 0 { '?' } else { '&' };
        write!(f, "{sep}{name}")?;
        if let Some(v) = value {
            write!(f, "={v}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_is_case_insensitive_and_first_match() {
        let params = Params::new()
            .with("Transport", Some("tcp"))
            .with("user", Some("phone"))
            .with("transport", Some("udp"))
            .with("lr", None);
        assert_eq!(params.get("transport"), Some(Some("tcp")));
        assert_eq!(params.get("USER"), Some(Some("phone")));
        assert_eq!(params.get("lr"), Some(None));
        assert_eq!(params.get("missing"), None);
        assert_eq!(params.len(), 4);
    }

    #[test]
    fn every_constructor_canonizes() {
        let pushed = Params::new().with("a;b", Some("%41@"));
        let collected: Params = [("a;b", Some("%41@"))]
            .into_iter()
            .collect();
        let converted = Params::from(vec![("a;b".to_string(), Some("%41@".to_string()))]);
        assert_eq!(pushed, collected);
        assert_eq!(pushed, converted);
        assert_eq!(
            pushed
                .iter()
                .collect::<Vec<_>>(),
            [("a%3Bb", Some("A%40"))]
        );
    }

    #[test]
    fn kinds_canonize_by_their_own_grammar() {
        let user = UserParams::new().with("n", Some("a=b"));
        let uri = Params::new().with("n", Some("a=b"));
        assert_eq!(user.get("n"), Some(Some("a=b")));
        assert_eq!(uri.get("n"), Some(Some("a%3Db")));
    }

    fn pairs(list: &Params) -> Vec<(&str, Option<&str>)> {
        list.iter()
            .collect()
    }

    #[test]
    fn remove_drops_every_case_insensitive_match() {
        let mut params = Params::new()
            .with("Lr", None)
            .with("transport", Some("tcp"))
            .with("lr", Some("x"));
        assert_eq!(params.remove("LR"), 2);
        assert_eq!(params.remove("lr"), 0);
        assert_eq!(pairs(&params), [("transport", Some("tcp"))]);
    }

    #[test]
    fn retain_keeps_order() {
        let mut params = Params::new()
            .with("a", Some("1"))
            .with("b", None)
            .with("c", Some("3"));
        params.retain(|_, value| value.is_some());
        assert_eq!(pairs(&params), [("a", Some("1")), ("c", Some("3"))]);
    }

    #[test]
    fn set_replaces_the_first_match_and_removes_the_rest() {
        let mut params = Params::new()
            .with("a", None)
            .with("Transport", Some("udp"))
            .with("b", None)
            .with("transport", Some("sctp"));
        params.set("TRANSPORT", Some("t c@p"));
        assert_eq!(
            pairs(&params),
            [("a", None), ("TRANSPORT", Some("t%20c%40p")), ("b", None)]
        );
        params.set("lr", None);
        assert_eq!(params.get("lr"), Some(None));
        assert_eq!(params.len(), 4);
        params.set("", None);
        assert_eq!(params.len(), 4);
        let mut set = Params::new();
        set.set("a;b", Some("%41"));
        assert_eq!(set, Params::new().with("a;b", Some("%41")));
    }

    #[test]
    fn lookups_match_the_name_as_pushed() {
        fn check<L: Default>(
            push: fn(&mut L, &str, Option<&str>),
            get: for<'a> fn(&'a L, &str) -> Option<Option<&'a str>>,
            remove: fn(&mut L, &str) -> usize,
        ) {
            let mut list = L::default();
            push(&mut list, "a b", Some("1"));
            push(&mut list, "%41", None);
            assert_eq!(get(&list, "a b"), Some(Some("1")));
            assert_eq!(get(&list, "%41"), Some(None));
            assert_eq!(get(&list, "a"), Some(None));
            assert_eq!(remove(&mut list, "a b"), 1);
            assert_eq!(remove(&mut list, "%61"), 1);
            assert_eq!(get(&list, "a b"), None);
        }
        check::<Params>(|l, n, v| l.push(n, v), Params::get, Params::remove);
        check::<UserParams>(|l, n, v| l.push(n, v), UserParams::get, UserParams::remove);
        check::<Headers>(|l, n, v| l.push(n, v), Headers::get, Headers::remove);
    }

    #[test]
    fn empty_name_without_value_is_dropped() {
        let headers = Headers::new()
            .with("", None)
            .with("", Some(""))
            .with("h", None);
        assert_eq!(
            headers
                .iter()
                .collect::<Vec<_>>(),
            [("", Some("")), ("h", None)]
        );
    }
}
