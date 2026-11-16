//! Fixed-work runs of the corpus for counting instructions: each workload
//! named on the command line runs a set number of rounds and exits, so
//! `perf stat -e instructions:u` on it gives a count that a loaded machine
//! does not move, unlike a timing. `scripts/bench-compare.sh` drives it.
//!
//! `cargo bench --bench count -- <workload>`; with no workload it lists
//! them.

use std::hint::black_box;

use sip_uri::{Redaction, SipUri, TelUri, Uri, UriEquivalence, UriParse, UriRedact, UrnUri};

mod corpus;

const ROUNDS: usize = 20_000;

fn sip_uris() -> Vec<SipUri> {
    corpus::SIP
        .iter()
        .filter_map(|s| SipUri::parse(s).ok())
        .collect()
}

const WORKLOADS: &[(&str, fn())] = &[
    ("parse_sip", || {
        for _ in 0..ROUNDS {
            for s in corpus::SIP {
                black_box(SipUri::parse(black_box(s)).ok());
            }
        }
    }),
    ("parse_sip_strict", || {
        for _ in 0..ROUNDS {
            for s in corpus::SIP {
                black_box(SipUri::parse_strict(black_box(s)).ok());
            }
        }
    }),
    ("parse_tel", || {
        for _ in 0..ROUNDS {
            for s in corpus::TEL {
                black_box(TelUri::parse(black_box(s)).ok());
            }
        }
    }),
    ("parse_urn", || {
        for _ in 0..ROUNDS {
            for s in corpus::URN {
                black_box(UrnUri::parse(black_box(s)).ok());
            }
        }
    }),
    ("parse_uri", || {
        for _ in 0..ROUNDS {
            for list in [corpus::SIP, corpus::TEL, corpus::URN] {
                for s in list {
                    black_box(Uri::parse(black_box(s)).ok());
                }
            }
        }
    }),
    ("parse_other", || {
        for _ in 0..ROUNDS {
            for s in corpus::OTHER {
                black_box(Uri::parse(black_box(s)).ok());
            }
        }
    }),
    ("lookup_param", || {
        let uri = SipUri::parse(corpus::LOOKUP_URI).ok();
        for _ in 0..ROUNDS * 10 {
            for name in corpus::LOOKUP_NAMES {
                black_box(
                    uri.as_ref()
                        .map(|u| u.param(black_box(name))),
                );
            }
        }
    }),
    ("display", || {
        let uris = sip_uris();
        for _ in 0..ROUNDS {
            for uri in &uris {
                black_box(uri.to_string());
            }
        }
    }),
    ("redacted", || {
        let uris = sip_uris();
        let how = Redaction::default();
        for _ in 0..ROUNDS {
            for uri in &uris {
                black_box(
                    uri.redacted(&how)
                        .to_string(),
                );
            }
        }
    }),
    ("equivalent", || {
        let uris = sip_uris();
        for _ in 0..ROUNDS * 10 {
            for pair in uris.windows(2) {
                black_box(pair[0].equivalent(&pair[1]));
            }
        }
    }),
];

fn main() {
    let names: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();
    if names.is_empty() {
        for (name, _) in WORKLOADS {
            println!("{name}");
        }
        return;
    }
    for name in &names {
        match WORKLOADS
            .iter()
            .find(|(n, _)| n == name)
        {
            Some((_, run)) => run(),
            None => {
                eprintln!("unknown workload {name}");
                std::process::exit(1);
            }
        }
    }
}
