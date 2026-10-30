//! Logical bytes to the URI text a builder takes, and back.
//!
//! Every component that builders and parts structs take as URI text has
//! one encoder and one decoder. An encoder escapes each byte its component
//! does not keep literal as uppercase `%XX` and cannot fail; its output is
//! already canonical, so the builder holds it unchanged and the matching
//! decoder gives the bytes back. A decoder turns every well-formed `%XX`
//! into its byte and copies a malformed `%` verbatim. It returns bytes,
//! never `str`, since an escape may stand for part of a UTF-8 sequence or
//! for none, and it borrows its input when that holds no `%`.
//!
//! Hostnames and URN NIDs have no pair: both are lowercased, so their bytes cannot round-trip.
//!
//! The cid pair is the one for text an [`OtherUri`](crate::OtherUri)
//! holds, because RFC 2392 defines how a cid: content-id decodes. It
//! escapes `/` as RFC 2392 §2 requires, so its output also fits either
//! part of a mid: URL.
//!
//! ```
//! use sip_uri_types::encoding::{decode_header, encode_header};
//! use sip_uri_types::{Host, SipUri};
//!
//! let replaces = encode_header("12345@example.com;to-tag=a");
//! assert_eq!(replaces, "12345%40example.com%3Bto-tag%3Da");
//! let uri = SipUri::new(Host::Hostname("example.com".into()))
//!     .with_header("Replaces", Some(&replaces));
//! let held = uri.header("Replaces").flatten().unwrap();
//! assert_eq!(held, replaces);
//! assert_eq!(decode_header(held).as_ref(), b"12345@example.com;to-tag=a");
//! ```

use std::borrow::Cow;

use super::{
    escape, escape_at, escape_q_start, is_fragment_literal, is_hnv_literal, is_nss_literal,
    is_param_literal, is_password_literal, is_pchar, is_rqf_literal, is_sip_fragment_literal,
    is_user_literal_or_hash, is_user_param_name_literal, ESCAPE_LEN,
};

fn decode(text: &str) -> Cow<'_, [u8]> {
    if !text.contains('%') {
        return Cow::Borrowed(text.as_bytes());
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match escape_at(bytes, i) {
            Some(b) => {
                out.push(b);
                i += ESCAPE_LEN;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    Cow::Owned(out)
}

macro_rules! component {
    ($(#[$doc:meta])* $encode:ident, $decode:ident, $escape:expr) => {
        /// Encode logical bytes as the text of:
        $(#[$doc])*
        pub fn $encode(value: impl AsRef<[u8]>) -> String {
            let escape: fn(&[u8]) -> String = $escape;
            escape(value.as_ref())
        }

        /// Decode to its logical bytes the text of:
        $(#[$doc])*
        pub fn $decode(text: &str) -> Cow<'_, [u8]> {
            decode(text)
        }
    };
}

component!(
    /// a SIP user part, as [`SipUri::with_user`](crate::SipUri::with_user)
    /// takes it.
    encode_user,
    decode_user,
    |v| escape(v, is_user_literal_or_hash)
);

component!(
    /// a user-param name, as [`UserParams`](crate::UserParams) takes it.
    encode_user_param_name,
    decode_user_param_name,
    |v| escape(v, is_user_param_name_literal)
);

component!(
    /// a user-param value, as [`UserParams`](crate::UserParams) takes it.
    encode_user_param_value,
    decode_user_param_value,
    |v| escape(v, is_user_literal_or_hash)
);

component!(
    /// a password, as [`SipUri::with_password`](crate::SipUri::with_password)
    /// takes it.
    encode_password,
    decode_password,
    |v| escape(v, is_password_literal)
);

component!(
    /// a SIP or tel: URI param name or value, as [`Params`](crate::Params)
    /// takes it.
    encode_param,
    decode_param,
    |v| escape(v, is_param_literal)
);

component!(
    /// a SIP URI header name or value, as [`Headers`](crate::Headers) takes
    /// it.
    encode_header,
    decode_header,
    |v| escape(v, is_hnv_literal)
);

component!(
    /// a tel: number, as [`TelUri::new`](crate::TelUri::new) takes it.
    encode_tel_number,
    decode_tel_number,
    |v| escape(v, is_user_literal_or_hash)
);

component!(
    /// a SIP fragment, as
    /// [`SipUri::with_fragment`](crate::SipUri::with_fragment) takes it.
    encode_sip_fragment,
    decode_sip_fragment,
    |v| escape(v, is_sip_fragment_literal)
);

component!(
    /// a tel: fragment or URN f-component, as
    /// [`TelUri::with_fragment`](crate::TelUri::with_fragment) and
    /// [`UrnUri::with_f_component`](crate::UrnUri::with_f_component) take it.
    encode_fragment,
    decode_fragment,
    |v| escape(v, is_fragment_literal)
);

component!(
    /// a URN NSS, as [`UrnUri::new`](crate::UrnUri::new) takes it.
    encode_nss,
    decode_nss,
    |v| escape(v, is_nss_literal)
);

component!(
    /// a URN r-component, as
    /// [`UrnUri::with_r_component`](crate::UrnUri::with_r_component) takes it.
    encode_r_component,
    decode_r_component,
    |v| escape_q_start(escape(v, is_rqf_literal))
);

component!(
    /// a URN q-component, as
    /// [`UrnUri::with_q_component`](crate::UrnUri::with_q_component) takes it.
    encode_q_component,
    decode_q_component,
    |v| escape(v, is_rqf_literal)
);

component!(
    /// an RFC 2392 cid: content-id or mid: URL part, as
    /// [`OtherUri::new`](crate::OtherUri::new) takes it after the scheme.
    ///
    /// `%25` decodes to `%`, per RFC 2392 erratum 454, which corrects the
    /// RFC's own example:
    ///
    /// ```
    /// use sip_uri_types::encoding::{decode_cid, encode_cid};
    ///
    /// assert_eq!(decode_cid("foo4%25foo1@example.net").as_ref(), b"foo4%foo1@example.net");
    /// assert_eq!(encode_cid("foo4%foo1@example.net"), "foo4%25foo1@example.net");
    /// ```
    encode_cid,
    decode_cid,
    |v| escape(v, is_pchar)
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Host, OtherUri, Pairs, SipUri, TelUri, UrnUri};

    type Held = fn(&str) -> Option<String>;

    struct Codec {
        name: &'static str,
        encode: fn(&[u8]) -> String,
        decode: fn(&str) -> Cow<'_, [u8]>,
        held: Vec<Held>,
    }

    fn sip() -> SipUri {
        SipUri::new(Host::Hostname("example.com".into()))
    }

    fn tel() -> TelUri {
        TelUri::new("+15551234567")
    }

    fn urn() -> UrnUri {
        UrnUri::new("example", "x")
    }

    fn owned(s: Option<&str>) -> Option<String> {
        s.map(str::to_owned)
    }

    fn only_name(mut pairs: Pairs<'_>) -> Option<String> {
        match (pairs.next(), pairs.next()) {
            (Some((name, _)), None) => Some(name.to_owned()),
            _ => None,
        }
    }

    fn codecs() -> Vec<Codec> {
        vec![
            Codec {
                name: "user",
                encode: |v| encode_user(v),
                decode: decode_user,
                held: vec![|t| {
                    owned(
                        sip()
                            .with_user(t)
                            .user(),
                    )
                }],
            },
            Codec {
                name: "user-param name",
                encode: |v| encode_user_param_name(v),
                decode: decode_user_param_name,
                held: vec![|t| {
                    only_name(
                        sip()
                            .with_user_param(t, None)
                            .user_params()
                            .iter(),
                    )
                }],
            },
            Codec {
                name: "user-param value",
                encode: |v| encode_user_param_value(v),
                decode: decode_user_param_value,
                held: vec![|t| {
                    owned(
                        sip()
                            .with_user_param("n", Some(t))
                            .user_param("n")
                            .flatten(),
                    )
                }],
            },
            Codec {
                name: "password",
                encode: |v| encode_password(v),
                decode: decode_password,
                held: vec![|t| {
                    owned(
                        sip()
                            .with_password(t)
                            .password(),
                    )
                }],
            },
            Codec {
                name: "param",
                encode: |v| encode_param(v),
                decode: decode_param,
                held: vec![
                    |t| {
                        only_name(
                            sip()
                                .with_param(t, None)
                                .params()
                                .iter(),
                        )
                    },
                    |t| {
                        owned(
                            sip()
                                .with_param("n", Some(t))
                                .param("n")
                                .flatten(),
                        )
                    },
                    |t| {
                        only_name(
                            tel()
                                .with_param(t, None)
                                .params()
                                .iter(),
                        )
                    },
                    |t| {
                        owned(
                            tel()
                                .with_param("n", Some(t))
                                .param("n")
                                .flatten(),
                        )
                    },
                ],
            },
            Codec {
                name: "header",
                encode: |v| encode_header(v),
                decode: decode_header,
                held: vec![
                    |t| {
                        only_name(
                            sip()
                                .with_header(t, None)
                                .headers()
                                .iter(),
                        )
                    },
                    |t| {
                        owned(
                            sip()
                                .with_header("n", Some(t))
                                .header("n")
                                .flatten(),
                        )
                    },
                ],
            },
            Codec {
                name: "tel number",
                encode: |v| encode_tel_number(v),
                decode: decode_tel_number,
                held: vec![|t| owned(TelUri::new(t).number())],
            },
            Codec {
                name: "sip fragment",
                encode: |v| encode_sip_fragment(v),
                decode: decode_sip_fragment,
                held: vec![|t| {
                    owned(
                        sip()
                            .with_fragment(t)
                            .fragment(),
                    )
                }],
            },
            Codec {
                name: "fragment",
                encode: |v| encode_fragment(v),
                decode: decode_fragment,
                held: vec![
                    |t| {
                        owned(
                            tel()
                                .with_fragment(t)
                                .fragment(),
                        )
                    },
                    |t| {
                        owned(
                            urn()
                                .with_f_component(t)
                                .f_component(),
                        )
                    },
                ],
            },
            Codec {
                name: "nss",
                encode: |v| encode_nss(v),
                decode: decode_nss,
                held: vec![|t| owned(UrnUri::new("example", t).nss())],
            },
            Codec {
                name: "r-component",
                encode: |v| encode_r_component(v),
                decode: decode_r_component,
                held: vec![|t| {
                    owned(
                        urn()
                            .with_r_component(t)
                            .r_component(),
                    )
                }],
            },
            Codec {
                name: "q-component",
                encode: |v| encode_q_component(v),
                decode: decode_q_component,
                held: vec![|t| {
                    owned(
                        urn()
                            .with_q_component(t)
                            .q_component(),
                    )
                }],
            },
            Codec {
                name: "cid",
                encode: |v| encode_cid(v),
                decode: decode_cid,
                held: vec![|t| {
                    OtherUri::new(Some("cid"), t)
                        .ok()
                        .map(|u| {
                            u.rest()
                                .to_owned()
                        })
                }],
            },
        ]
    }

    #[test]
    fn cid_escapes_the_slash_a_mid_url_reserves() {
        assert_eq!(encode_cid("a/b@example.net"), "a%2Fb@example.net");
    }

    #[test]
    fn every_byte_encodes_literal_or_as_uppercase_escape_and_decodes_back() {
        let all: Vec<u8> = (0..=u8::MAX).collect();
        for c in codecs() {
            for b in 0..=u8::MAX {
                let text = (c.encode)(&[b]);
                assert!(
                    text.as_bytes() == [b] || text == format!("%{b:02X}"),
                    "{}: {b:#04x} as {text:?}",
                    c.name
                );
                assert_eq!((c.decode)(&text).as_ref(), [b], "{}: {text:?}", c.name);
            }
            assert_eq!((c.decode)(&(c.encode)(&all)).as_ref(), all, "{}", c.name);
        }
    }

    #[test]
    fn builders_hold_encoder_output_unchanged() {
        let mut inputs: Vec<Vec<u8>> = (0..=u8::MAX)
            .map(|b| vec![b])
            .collect();
        for a in 0..0x80u8 {
            for b in 0..0x80u8 {
                inputs.push(vec![a, b]);
            }
        }
        for c in codecs() {
            for input in &inputs {
                let text = (c.encode)(input);
                for held in &c.held {
                    assert_eq!(held(&text).as_ref(), Some(&text), "{}: {input:?}", c.name);
                }
            }
        }
    }

    #[test]
    fn decode_borrows_text_without_escapes() {
        assert!(matches!(decode(""), Cow::Borrowed(b"")));
        assert!(matches!(
            decode("+15551234567;cpc=emergency"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn decode_reads_either_hex_case_and_copies_a_malformed_percent() {
        assert_eq!(decode("%2b%3B").as_ref(), b"+;");
        assert_eq!(decode("%FF").as_ref(), b"\xFF");
        for malformed in ["100%", "a%2", "%zz", "%%41"] {
            assert_eq!(
                decode(malformed).as_ref(),
                malformed
                    .replace("%41", "A")
                    .as_bytes()
            );
        }
    }
}
