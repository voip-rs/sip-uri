#!/bin/bash
# Checkout the release tags' commit, publish each tagged crate to crates.io,
# sip-uri-diagnostics before sip-uri-types before sip-uri, and return to the
# branch.
#
# Usage: scripts/release-publish.sh <tag> [<tag>]...
# Tags: vX.Y.Z[-rc.N] for sip-uri, sip-uri-types-vX.Y.Z[-rc.N] for
# sip-uri-types, sip-uri-diagnostics-vX.Y.Z[-rc.N] for sip-uri-diagnostics.

set -e

if [ $# -eq 0 ]; then
	echo "Usage: $0 <tag> [<tag>]..." >&2
	exit 1
fi

DIAGNOSTICS_TAG=""
TYPES_TAG=""
URI_TAG=""
for tag in "$@"; do
	if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
		echo "Tag $tag not found locally. Push/fetch it first." >&2
		exit 1
	fi
	case "$tag" in
	sip-uri-diagnostics-v*) DIAGNOSTICS_TAG="$tag" ;;
	sip-uri-types-v*) TYPES_TAG="$tag" ;;
	v*) URI_TAG="$tag" ;;
	*)
		echo "Tag $tag names none of sip-uri (vX.Y.Z), sip-uri-types (sip-uri-types-vX.Y.Z) or sip-uri-diagnostics (sip-uri-diagnostics-vX.Y.Z)." >&2
		exit 1
		;;
	esac
done

COMMIT="$(git rev-parse "$1^{commit}")"
for tag in "$@"; do
	if [ "$(git rev-parse "$tag^{commit}")" != "$COMMIT" ]; then
		echo "Tags $1 and $tag point at different commits." >&2
		exit 1
	fi
done

BRANCH="$(git symbolic-ref --short HEAD)"
git checkout "$COMMIT"

# Tag name and manifest version must agree before anything is uploaded.
PACKAGES=()
for pair in "sip-uri-diagnostics:${DIAGNOSTICS_TAG#sip-uri-diagnostics-v}:$DIAGNOSTICS_TAG" "sip-uri-types:${TYPES_TAG#sip-uri-types-v}:$TYPES_TAG" "sip-uri:${URI_TAG#v}:$URI_TAG"; do
	IFS=: read -r crate version tag <<<"$pair"
	[ -n "$tag" ] || continue
	manifest="$(cargo metadata --no-deps --format-version 1 | jq -r --arg n "$crate" '.packages[] | select(.name == $n) | .version')"
	if [ "$manifest" != "$version" ]; then
		echo "$tag says $version but $crate's Cargo.toml says $manifest." >&2
		git switch "$BRANCH"
		exit 1
	fi
	PACKAGES+=("$crate")
done

# Cargo reuses a registry crate's build by version alone, so verifying in the
# shared target dir can build sip-uri against an older sip-uri-types 1.0.0.
VERIFY_DIR=target/release-verify
rm -rf "$VERIFY_DIR"
PACKAGE_ARGS=()
for crate in "${PACKAGES[@]}"; do
	PACKAGE_ARGS+=(-p "$crate")
done
cargo publish --dry-run --target-dir "$VERIFY_DIR" "${PACKAGE_ARGS[@]}"
for crate in "${PACKAGES[@]}"; do
	cargo publish --target-dir "$VERIFY_DIR" -p "$crate"
done
git switch "$BRANCH"

echo "Published $*."
