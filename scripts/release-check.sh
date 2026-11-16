#!/bin/bash
# Pre-release checks: fmt, clippy, docs, tests, publish dry-run.
# Run on a clean master before scripts/release-tag.sh, which runs
# semver-checks once the new versions are in the manifests.

set -e

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" cargo doc --workspace --all-features --no-deps
cargo test --workspace --all-features --release

# Packages every crate in dependency order, so sip-uri verifies against the
# sip-uri-types and sip-uri-diagnostics being released rather than the ones
# on crates.io.
cargo publish --workspace --dry-run

echo "Pre-release checks passed"
