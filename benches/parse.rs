//! Parse, lookup, render and compare costs over the synthetic corpus in
//! `corpus.rs`, to catch a regression between releases.
//!
//! Compare a change against its base with criterion baselines: on the base,
//! `cargo bench --bench parse -- --save-baseline base`; on the change,
//! `cargo bench --bench parse -- --baseline base`. Pin the core
//! (`taskset -c N`) and rerun before believing a difference of a few
//! percent: the machine drifts more than most changes cost.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use sip_uri::{Redaction, SipUri, TelUri, Uri, UriEquivalence, UriParse, UriRedact, UrnUri};

mod corpus;

fn parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse");
    let all: Vec<&str> = [corpus::SIP, corpus::TEL, corpus::URN, corpus::OTHER].concat();

    group.throughput(Throughput::Elements(corpus::SIP.len() as u64));
    group.bench_function("sip", |b| {
        b.iter(|| {
            for s in corpus::SIP {
                black_box(SipUri::parse(black_box(s)).ok());
            }
        })
    });
    group.bench_function("sip_strict", |b| {
        b.iter(|| {
            for s in corpus::SIP {
                black_box(SipUri::parse_strict(black_box(s)).ok());
            }
        })
    });
    group.throughput(Throughput::Elements(corpus::TEL.len() as u64));
    group.bench_function("tel", |b| {
        b.iter(|| {
            for s in corpus::TEL {
                black_box(TelUri::parse(black_box(s)).ok());
            }
        })
    });
    group.throughput(Throughput::Elements(corpus::URN.len() as u64));
    group.bench_function("urn", |b| {
        b.iter(|| {
            for s in corpus::URN {
                black_box(UrnUri::parse(black_box(s)).ok());
            }
        })
    });
    group.throughput(Throughput::Elements(all.len() as u64));
    group.bench_function("uri_mixed", |b| {
        b.iter(|| {
            for s in &all {
                black_box(Uri::parse(black_box(s)).ok());
            }
        })
    });
    group.finish();
}

fn lookup(c: &mut Criterion) {
    let uri = SipUri::parse(corpus::LOOKUP_URI).expect("corpus URI parses");
    let mut group = c.benchmark_group("lookup");
    group.throughput(Throughput::Elements(corpus::LOOKUP_NAMES.len() as u64));
    group.bench_function("param", |b| {
        b.iter(|| {
            for name in corpus::LOOKUP_NAMES {
                black_box(uri.param(black_box(name)));
            }
        })
    });
    group.bench_function("header", |b| {
        b.iter(|| black_box(uri.header(black_box("priority"))))
    });
    group.finish();
}

fn render(c: &mut Criterion) {
    let uris: Vec<SipUri> = corpus::SIP
        .iter()
        .filter_map(|s| SipUri::parse(s).ok())
        .collect();
    let how = Redaction::default();
    let mut group = c.benchmark_group("render");
    group.throughput(Throughput::Elements(uris.len() as u64));
    group.bench_function("display", |b| {
        b.iter(|| {
            for uri in &uris {
                black_box(uri.to_string());
            }
        })
    });
    group.bench_function("redacted", |b| {
        b.iter(|| {
            for uri in &uris {
                black_box(
                    uri.redacted(&how)
                        .to_string(),
                );
            }
        })
    });
    group.bench_function("equivalent", |b| {
        b.iter(|| {
            for pair in uris.windows(2) {
                black_box(pair[0].equivalent(&pair[1]));
            }
        })
    });
    group.finish();
}

criterion_group!(benches, parse, lookup, render);
criterion_main!(benches);
