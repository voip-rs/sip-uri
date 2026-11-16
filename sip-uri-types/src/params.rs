//! Ordered `(name, value)` lists for params, user-params and headers.
//!
//! A list holds every name and value back to back in one `String`, with one
//! span per pair recording, as `u32` offsets, where its name ends and where
//! its value ends, if it has one; a pair starts where the one before it
//! ends. That is one allocation per list instead of one per name and value,
//! and canonical text is appended without an intermediate copy. The storage
//! is private, as the rationale's opaque-collections rule allows. The buffer
//! stays compact and in pair order, so the derived `Eq` and `Hash` compare
//! the pairs in order and nothing else.
//!
//! The readers in `storage` slice names and values out of the buffer
//! unchecked. That is sound because spans are written only by `storage`'s
//! own appends, which record each offset after the text it ends, so offsets
//! rise, lie on char boundaries and never exceed the buffer's length;
//! rebuilding copies spans and text together. An offset is recorded only
//! through `u32::try_from`: that check is load-bearing, and replacing it with
//! `as` would let a wrapped offset read out of bounds.
//!
//! A list's text is therefore capped at `u32::MAX` bytes, 4 GiB. No such list
//! comes off the wire, where a SIP message is at most 64 KB over UDP and
//! proxies cap TCP far lower; only a program building one by hand reaches
//! it. Each path that appends meets the cap its own way, the list left
//! unchanged: the parser drops the pair with a warning, deserialization
//! fails with an error naming the field, `try_push` and `try_with` return
//! [`CapacityError`], and `push`, `with`, `set`, `From` and `FromIterator`
//! panic, as `Vec` does past its capacity.
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

#[cfg(all(test, feature = "serde"))]
pub(crate) use storage::limit;
use storage::PairBuf;
pub use storage::Pairs;

/// A list's text would exceed `u32::MAX` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapacityError;

impl fmt::Display for CapacityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("param or header list text exceeds u32::MAX bytes")
    }
}

impl std::error::Error for CapacityError {}

/// Panic on a list's capacity overflow: the infallible appenders' half of
/// the one check.
#[track_caller]
fn room(appended: Result<(), CapacityError>) {
    if let Err(e) = appended {
        panic!("{e}");
    }
}

/// How one kind of list canonizes what it holds and the names it looks up,
/// a type so that each kind's calls are static and inline.
trait Grammar {
    fn push_name(out: &mut String, name: &str);
    fn push_value(out: &mut String, value: &str);
    fn lookup_name(name: &str) -> Cow<'_, str>;
}

enum ParamGrammar {}

impl Grammar for ParamGrammar {
    fn push_name(out: &mut String, name: &str) {
        canon::push_param(out, name);
    }

    fn push_value(out: &mut String, value: &str) {
        canon::push_param(out, value);
    }

    fn lookup_name(name: &str) -> Cow<'_, str> {
        canon::param_name(name)
    }
}

enum UserParamGrammar {}

impl Grammar for UserParamGrammar {
    fn push_name(out: &mut String, name: &str) {
        canon::push_user_param_name(out, name);
    }

    fn push_value(out: &mut String, value: &str) {
        canon::push_user_param_value(out, value);
    }

    fn lookup_name(name: &str) -> Cow<'_, str> {
        canon::user_param_name(name)
    }
}

enum HeaderGrammar {}

impl Grammar for HeaderGrammar {
    fn push_name(out: &mut String, name: &str) {
        canon::push_header(out, name);
    }

    fn push_value(out: &mut String, value: &str) {
        canon::push_header(out, value);
    }

    fn lookup_name(name: &str) -> Cow<'_, str> {
        canon::header_name(name)
    }
}

/// Whether a held name matches `name` ignoring ASCII case, trying the exact
/// compare first since a name is usually looked up as it is held.
#[inline]
fn same_name(held: &str, name: &str) -> bool {
    same_name_bytes(held.as_bytes(), name.as_bytes())
}

#[inline(always)]
fn same_name_bytes(held: &[u8], name: &[u8]) -> bool {
    held == name || held.eq_ignore_ascii_case(name)
}

/// The buffer, its spans and the only code that writes or slices them.
#[allow(unsafe_code)]
mod storage {
    use std::fmt;

    use super::{same_name_bytes, CapacityError};

    /// Where one pair ends in its list's buffer.
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    struct Span {
        name_end: u32,
        value_end: Option<u32>,
    }

    impl Span {
        #[inline]
        fn end(self) -> u32 {
            self.value_end
                .unwrap_or(self.name_end)
        }
    }

    /// `len` as a span offset, the one check keeping every offset in range.
    #[inline]
    pub(super) fn offset(len: usize) -> Result<u32, CapacityError> {
        #[cfg(test)]
        if len > limit::get() {
            return Err(CapacityError);
        }
        u32::try_from(len).map_err(|_| CapacityError)
    }

    /// A lower cap than `u32::MAX` for the current test thread, so the
    /// overflow paths run without 4 GiB of text.
    #[cfg(test)]
    pub(crate) mod limit {
        use std::cell::Cell;

        thread_local! {
            static LIMIT: Cell<usize> = const { Cell::new(usize::MAX) };
        }

        pub(crate) fn get() -> usize {
            LIMIT.with(Cell::get)
        }

        /// Run `f` with list text capped at `bytes`.
        pub(crate) fn with<T>(bytes: usize, f: impl FnOnce() -> T) -> T {
            let before = LIMIT.with(|l| l.replace(bytes));
            let out = f();
            LIMIT.with(|l| l.set(before));
            out
        }
    }

    /// Bytes a list's buffer starts with, enough for a typical list of a few
    /// params, so that the first pairs do not each grow it.
    const FIRST_CAPACITY: usize = 32;

    /// Iterator over the `(name, value)` pairs of a [`Params`](super::Params),
    /// [`UserParams`](super::UserParams) or [`Headers`](super::Headers), in
    /// order.
    #[derive(Clone)]
    pub struct Pairs<'a> {
        text: &'a str,
        /// Where the first remaining pair starts in `text`.
        start: u32,
        spans: &'a [Span],
    }

    impl<'a> Iterator for Pairs<'a> {
        type Item = (&'a str, Option<&'a str>);

        #[inline]
        fn next(&mut self) -> Option<Self::Item> {
            let (span, spans) = self
                .spans
                .split_first()?;
            // SAFETY: text and spans come from one PairBuf (only `iter` builds a
            // Pairs), and start is the end of the span consumed before this one.
            let pair = unsafe { pair(self.text, self.start, *span) };
            self.start = span.end();
            self.spans = spans;
            Some(pair)
        }

        #[inline]
        fn size_hint(&self) -> (usize, Option<usize>) {
            (
                self.spans
                    .len(),
                Some(
                    self.spans
                        .len(),
                ),
            )
        }
    }

    impl DoubleEndedIterator for Pairs<'_> {
        #[inline]
        fn next_back(&mut self) -> Option<Self::Item> {
            let (span, spans) = self
                .spans
                .split_last()?;
            let start = spans
                .last()
                .map_or(self.start, |before| before.end());
            self.spans = spans;
            // SAFETY: as in `next`; start is the end of the span before this one,
            // or the front's start when none remains before it.
            Some(unsafe { pair(self.text, start, *span) })
        }
    }

    impl ExactSizeIterator for Pairs<'_> {}

    impl fmt::Debug for Pairs<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_list()
                .entries(self.clone())
                .finish()
        }
    }

    /// The name and value of `span`, whose pair starts at `start`.
    ///
    /// # Safety
    ///
    /// `span` is one of the spans of the list whose buffer is `text`, and
    /// `start` is the end of the span before it in that list, or 0 for the
    /// first: [`PairBuf::append`] records them in order, on char boundaries
    /// of the buffer, none past its length, so the slices below are in range.
    #[inline]
    unsafe fn pair(text: &str, start: u32, span: Span) -> (&str, Option<&str>) {
        // SAFETY: the caller's contract puts start..name_end on char boundaries
        // within text.
        let name = unsafe { text.get_unchecked(start as usize..span.name_end as usize) };
        let value = span
            .value_end
            // SAFETY: as above; a value runs from its name's end to its own.
            .map(|end| unsafe { text.get_unchecked(span.name_end as usize..end as usize) });
        (name, value)
    }

    /// The storage and editing every list kind shares.
    #[derive(Clone, Default, PartialEq, Eq, Hash)]
    pub(super) struct PairBuf {
        text: String,
        spans: Vec<Span>,
    }

    impl PairBuf {
        #[inline]
        pub(super) fn len(&self) -> usize {
            self.spans
                .len()
        }

        #[inline]
        pub(super) fn iter(&self) -> Pairs<'_> {
            Pairs {
                text: &self.text,
                start: 0,
                spans: &self.spans,
            }
        }

        /// Grow the buffer for `needed` more bytes, to at least
        /// `FIRST_CAPACITY` on the first pair.
        #[inline]
        pub(super) fn reserve(&mut self, needed: usize) {
            let room = if self
                .text
                .capacity()
                == 0
            {
                needed.max(FIRST_CAPACITY)
            } else {
                needed
            };
            self.text
                .reserve(room);
        }

        /// Append a pair already canonical, held by a list no longer than
        /// this one will be.
        pub(super) fn push_held(&mut self, name: &str, value: Option<&str>) {
            super::room(self.append(name, value, String::push_str, String::push_str));
        }

        /// Append a pair, each part written by its own function; past the
        /// cap the buffer is cut back and the list left as it was.
        ///
        /// `push_name` and `push_value` may only append to the buffer: one
        /// that removed or rewrote text before its start would leave earlier
        /// spans past their text, which [`pair`] then reads unchecked.
        #[inline]
        pub(super) fn append(
            &mut self,
            name: &str,
            value: Option<&str>,
            push_name: impl FnOnce(&mut String, &str),
            push_value: impl FnOnce(&mut String, &str),
        ) -> Result<(), CapacityError> {
            let start = self
                .text
                .len();
            push_name(&mut self.text, name);
            let name_end = offset(
                self.text
                    .len(),
            );
            let value_end = value
                .map(|v| {
                    push_value(&mut self.text, v);
                    offset(
                        self.text
                            .len(),
                    )
                })
                .transpose();
            let (Ok(name_end), Ok(value_end)) = (name_end, value_end) else {
                self.text
                    .truncate(start);
                return Err(CapacityError);
            };
            self.spans
                .push(Span {
                    name_end,
                    value_end,
                });
            Ok(())
        }

        /// The value of the first pair whose held name matches `name`,
        /// skipping a name of another length unsliced since neither compare
        /// can match it.
        #[inline]
        pub(super) fn find(&self, name: &str) -> Option<Option<&str>> {
            let mut start = 0;
            for span in &self.spans {
                if (span.name_end - start) as usize == name.len() {
                    // SAFETY: span is this list's own, and start the end of the one
                    // before it, tracked by this loop from 0.
                    let (held, value) = unsafe { pair(&self.text, start, *span) };
                    if same_name_bytes(held.as_bytes(), name.as_bytes()) {
                        return Some(value);
                    }
                }
                start = span.end();
            }
            None
        }

        /// The first `n` pairs, with room for the rest of the buffer.
        pub(super) fn prefix(&self, n: usize) -> PairBuf {
            let end = n
                .checked_sub(1)
                .map_or(0, |last| self.spans[last].end());
            let mut text = String::with_capacity(
                self.text
                    .len(),
            );
            text.push_str(&self.text[..end as usize]);
            let mut spans = Vec::with_capacity(
                self.spans
                    .len(),
            );
            spans.extend_from_slice(&self.spans[..n]);
            PairBuf { text, spans }
        }

        /// Every span lies in order on char boundaries within the text.
        #[cfg(test)]
        pub(super) fn check_spans(&self) {
            let mut start = 0;
            for span in &self.spans {
                let ends = [Some(span.name_end), span.value_end];
                for end in ends
                    .into_iter()
                    .flatten()
                {
                    assert!(start <= end, "span ends before it starts");
                    assert!(
                        end as usize
                            <= self
                                .text
                                .len(),
                        "span ends past the text"
                    );
                    assert!(
                        self.text
                            .is_char_boundary(end as usize),
                        "span splits a char"
                    );
                    start = end;
                }
            }
        }
    }
}

impl PairBuf {
    /// Append the canonized pair, unless it is a valueless empty name: its
    /// bare delimiter parses as nothing.
    #[inline]
    fn push<G: Grammar>(&mut self, name: &str, value: Option<&str>) -> Result<(), CapacityError> {
        if name.is_empty() && value.is_none() {
            return Ok(());
        }
        self.reserve(name.len() + value.map_or(0, str::len));
        self.append(name, value, G::push_name, G::push_value)
    }

    fn get<G: Grammar>(&self, name: &str) -> Option<Option<&str>> {
        if self.len() == 0 {
            return None;
        }
        if let Some(value) = self.find(name) {
            return Some(value);
        }
        match G::lookup_name(name) {
            Cow::Owned(canonical) => self.find(&canonical),
            Cow::Borrowed(_) => None,
        }
    }

    fn remove<G: Grammar>(&mut self, name: &str) -> usize {
        let held = self
            .iter()
            .any(|(n, _)| same_name(n, name));
        let name = if held {
            Cow::Borrowed(name)
        } else {
            G::lookup_name(name)
        };
        let before = self.len();
        self.retain(|n, _| !same_name(n, &name));
        before - self.len()
    }

    /// Keep the pairs `keep` accepts, rebuilding the buffer from the first
    /// pair it drops, so a call dropping nothing allocates nothing.
    fn retain(&mut self, mut keep: impl FnMut(&str, Option<&str>) -> bool) {
        let mut rebuilt: Option<PairBuf> = None;
        for (i, (name, value)) in self
            .iter()
            .enumerate()
        {
            let kept = keep(name, value);
            match &mut rebuilt {
                Some(list) if kept => list.push_held(name, value),
                None if !kept => rebuilt = Some(self.prefix(i)),
                _ => {}
            }
        }
        if let Some(rebuilt) = rebuilt {
            *self = rebuilt;
        }
    }

    /// Append the canonized pair, then move it over the first earlier pair
    /// of the same name and drop every later one.
    fn set<G: Grammar>(&mut self, name: &str, value: Option<&str>) -> Result<(), CapacityError> {
        let held = self.len();
        self.push::<G>(name, value)?;
        let mut pairs = self.iter();
        // None when push dropped a valueless empty name.
        let Some((name, value)) = pairs
            .next_back()
            .filter(|_| pairs.len() == held)
        else {
            return Ok(());
        };
        let Some(first) = pairs.position(|(n, _)| same_name(n, name)) else {
            return Ok(());
        };
        let mut rebuilt = self.prefix(first);
        rebuilt.push_held(name, value);
        for (n, v) in self
            .iter()
            .take(held)
            .skip(first + 1)
        {
            if !same_name(n, name) {
                rebuilt.push_held(n, v);
            }
        }
        *self = rebuilt;
        Ok(())
    }
}

macro_rules! pair_list {
    ($(#[$doc:meta])* $name:ident, $grammar:ty) => {
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
        pub struct $name(PairBuf);

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
                self.len() == 0
            }

            /// The `(name, value)` pairs, in order.
            pub fn iter(&self) -> Pairs<'_> {
                self.0
                    .iter()
            }

            /// The value of the first pair named `name`, canonized and
            /// compared case-insensitively: `Some(None)` when it has no value.
            pub fn get(&self, name: &str) -> Option<Option<&str>> {
                self.0
                    .get::<$grammar>(name)
            }

            /// Append a pair, canonized.
            ///
            /// # Panics
            ///
            /// When the list's text would exceed `u32::MAX` bytes;
            /// [`try_push`](Self::try_push) returns the error instead.
            #[track_caller]
            pub fn push(&mut self, name: impl AsRef<str>, value: Option<&str>) {
                room(self.try_push(name, value));
            }

            /// Append a pair, canonized, or leave the list unchanged when its
            /// text would exceed `u32::MAX` bytes.
            pub fn try_push(
                &mut self,
                name: impl AsRef<str>,
                value: Option<&str>,
            ) -> Result<(), CapacityError> {
                self.0
                    .push::<$grammar>(name.as_ref(), value)
            }

            /// Append a pair, canonized, and return the list.
            ///
            /// # Panics
            ///
            /// When the list's text would exceed `u32::MAX` bytes;
            /// [`try_with`](Self::try_with) returns the error instead.
            #[track_caller]
            pub fn with(mut self, name: impl AsRef<str>, value: Option<&str>) -> Self {
                self.push(name, value);
                self
            }

            /// Append a pair, canonized, and return the list, or fail when its
            /// text would exceed `u32::MAX` bytes.
            pub fn try_with(
                mut self,
                name: impl AsRef<str>,
                value: Option<&str>,
            ) -> Result<Self, CapacityError> {
                self.try_push(name, value)?;
                Ok(self)
            }

            /// Remove every pair named `name`, canonized and compared
            /// case-insensitively, and return how many were removed.
            pub fn remove(&mut self, name: &str) -> usize {
                self.0
                    .remove::<$grammar>(name)
            }

            /// Keep only the pairs for which `keep` returns `true`, in order.
            pub fn retain(&mut self, keep: impl FnMut(&str, Option<&str>) -> bool) {
                self.0
                    .retain(keep);
            }

            /// Set the pair named `name`, canonized: the first pair whose
            /// canonical name matches case-insensitively is replaced in place
            /// and every later match removed, so the name is left once; with
            /// no match the pair is appended. An empty name without a value is
            /// dropped, as by [`push`](Self::push).
            ///
            /// # Panics
            ///
            /// When the list's text would exceed `u32::MAX` bytes.
            #[track_caller]
            pub fn set(&mut self, name: impl AsRef<str>, value: Option<&str>) {
                room(
                    self.0
                        .set::<$grammar>(name.as_ref(), value),
                );
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
            /// # Panics
            ///
            /// When the list's text would exceed `u32::MAX` bytes.
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
            /// # Panics
            ///
            /// When the list's text would exceed `u32::MAX` bytes.
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
    ParamGrammar
);

pair_list!(
    /// Parameters inside a SIP URI's userinfo, after the user part.
    UserParams,
    UserParamGrammar
);

pair_list!(
    /// Headers of a SIP URI, after `?`. A header written without `=` has no
    /// value, and one written `name=` has an empty one.
    Headers,
    HeaderGrammar
);

/// Write each pair as `;name` or `;name=value`.
pub(crate) fn format_params(pairs: Pairs<'_>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (name, value) in pairs {
        f.write_str(";")?;
        f.write_str(name)?;
        if let Some(v) = value {
            f.write_str("=")?;
            f.write_str(v)?;
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
        let sep = if i == 0 { "?" } else { "&" };
        f.write_str(sep)?;
        f.write_str(name)?;
        if let Some(v) = value {
            f.write_str("=")?;
            f.write_str(v)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names and values that canonize to escapes, multi-byte text among them.
    const TEXTS: &[&str] = &["a", "", "é;x", "日本", "%41", "a b", "Ab", "=@&?"];

    /// Run every mutating operation over lists built from [`TEXTS`],
    /// checking the spans after each.
    #[allow(clippy::type_complexity)]
    fn check_every_edit<L: Default + Clone>(
        push: fn(&mut L, &str, Option<&str>),
        set: fn(&mut L, &str, Option<&str>),
        remove: fn(&mut L, &str) -> usize,
        retain: fn(&mut L, &mut dyn FnMut(&str, Option<&str>) -> bool),
        collect: fn(Vec<(String, Option<String>)>) -> L,
        buf: fn(&L) -> &PairBuf,
    ) {
        let pairs: Vec<(String, Option<String>)> = TEXTS
            .iter()
            .flat_map(|name| {
                [None]
                    .into_iter()
                    .chain(
                        TEXTS
                            .iter()
                            .map(|v| Some(v.to_string())),
                    )
                    .map(move |value| (name.to_string(), value))
            })
            .collect();
        let check = |list: &L| {
            buf(list).check_spans();
        };
        let built = collect(pairs.clone());
        check(&built);
        let mut list = L::default();
        for (name, value) in &pairs {
            push(&mut list, name, value.as_deref());
            check(&list);
        }
        for (i, (name, value)) in pairs
            .iter()
            .enumerate()
        {
            let mut edited = list.clone();
            set(&mut edited, name, value.as_deref());
            check(&edited);
            set(&mut edited, &format!("new{i}"), Some("é"));
            check(&edited);
            remove(&mut edited, name);
            check(&edited);
            let mut n = 0;
            retain(&mut edited, &mut |_, _| {
                n += 1;
                n % 3 != i % 3
            });
            check(&edited);
        }
        retain(&mut list, &mut |_, _| false);
        check(&list);
    }

    #[test]
    fn spans_stay_in_order_on_char_boundaries_after_every_edit() {
        check_every_edit::<Params>(
            |l, n, v| l.push(n, v),
            |l, n, v| l.set(n, v),
            Params::remove,
            |l, keep| l.retain(keep),
            Params::from,
            |l| &l.0,
        );
        check_every_edit::<UserParams>(
            |l, n, v| l.push(n, v),
            |l, n, v| l.set(n, v),
            UserParams::remove,
            |l, keep| l.retain(keep),
            UserParams::from,
            |l| &l.0,
        );
        check_every_edit::<Headers>(
            |l, n, v| l.push(n, v),
            |l, n, v| l.set(n, v),
            Headers::remove,
            |l, keep| l.retain(keep),
            |pairs| {
                pairs
                    .into_iter()
                    .collect()
            },
            |l| &l.0,
        );
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn an_offset_past_u32_max_is_refused() {
        let max = u32::MAX as usize;
        assert_eq!(storage::offset(max), Ok(u32::MAX));
        assert_eq!(storage::offset(max + 1), Err(CapacityError));
        assert_eq!(storage::offset(usize::MAX), Err(CapacityError));
    }

    #[test]
    fn a_pair_past_the_limit_leaves_the_list_unchanged() {
        let list = Params::new().with("a", Some("1"));
        storage::limit::with(5, || {
            let mut grown = list.clone();
            assert_eq!(grown.try_push("bb", Some("22")), Err(CapacityError));
            assert_eq!(grown, list);
            grown
                .0
                .check_spans();
            assert_eq!(grown.try_push("é", None), Err(CapacityError));
            assert_eq!(grown, list);
            assert_eq!(
                grown
                    .clone()
                    .try_with("b", Some("2")),
                Ok(list
                    .clone()
                    .with("b", Some("2")))
            );
        });
    }

    #[test]
    #[should_panic(expected = "exceeds u32::MAX bytes")]
    fn push_past_the_limit_panics() {
        storage::limit::with(4, || Params::new().with("long", Some("value")));
    }

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
    fn edits_leave_the_list_equal_to_one_built_fresh() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        fn hash(list: &Params) -> u64 {
            let mut hasher = DefaultHasher::new();
            list.hash(&mut hasher);
            hasher.finish()
        }
        let mut edited = Params::new()
            .with("a", Some("1"))
            .with("drop", None)
            .with("b", Some("2"))
            .with("B", Some("3"))
            .with("c", None);
        edited.retain(|name, _| name != "drop");
        edited.set("b", Some("4"));
        assert_eq!(edited.remove("c"), 1);
        let fresh: Params = pairs(&edited)
            .into_iter()
            .collect();
        assert_eq!(pairs(&edited), [("a", Some("1")), ("b", Some("4"))]);
        assert_eq!(edited, fresh);
        assert_eq!(hash(&edited), hash(&fresh));
    }

    #[test]
    fn pairs_iterate_from_both_ends() {
        let params = Params::new()
            .with("a", Some("1"))
            .with("b", None)
            .with("c", Some(""))
            .with("d", Some("4"));
        let mut it = params.iter();
        assert_eq!(it.next_back(), Some(("d", Some("4"))));
        assert_eq!(it.next(), Some(("a", Some("1"))));
        assert_eq!(it.len(), 2);
        assert_eq!(format!("{it:?}"), r#"[("b", None), ("c", Some(""))]"#);
        assert_eq!(it.next_back(), Some(("c", Some(""))));
        assert_eq!(it.next_back(), Some(("b", None)));
        assert_eq!(it.next(), None);
        assert_eq!(
            params
                .iter()
                .rev()
                .collect::<Vec<_>>(),
            [
                ("d", Some("4")),
                ("c", Some("")),
                ("b", None),
                ("a", Some("1"))
            ]
        );
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
