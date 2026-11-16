#!/bin/bash
# Bump each crate's version, commit, pin Cargo.lock on a detached commit,
# sign one tag per crate on it.
#
# The detach dance is the error-prone part of a release: a chained command
# rejected mid-way (a hook, a denied permission) can commit Cargo.lock onto
# the branch it must never reach. Scripting it removes the chaining risk;
# each step here is checked before the next runs.
#
# Usage: scripts/release-tag.sh <crate> <X.Y.Z[-rc.N]> <changelog-file> [...]
# Crates: sip-uri (tag vX.Y.Z), sip-uri-types (tag sip-uri-types-vX.Y.Z) and
# sip-uri-diagnostics (tag sip-uri-diagnostics-vX.Y.Z).

set -e

if [ $# -eq 0 ] || [ $(($# % 3)) -ne 0 ]; then
	echo "Usage: $0 <crate> <X.Y.Z[-rc.N]> <changelog-file> [<crate> <version> <changelog-file>]..." >&2
	exit 1
fi

BRANCH="$(git symbolic-ref --short HEAD)"
if [ "$BRANCH" != "master" ]; then
	echo "Must be on master (currently on $BRANCH)." >&2
	exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
	echo "Working tree is dirty; commit or stash first." >&2
	git status --short >&2
	exit 1
fi

CRATES=()
VERSIONS=()
TAGS=()
CHANGELOGS=()
SUBJECT=""
while [ $# -gt 0 ]; do
	CRATE="$1" VERSION="$2" CHANGELOG_FILE="$3"
	shift 3

	case "$CRATE" in
	sip-uri) TAG="v$VERSION" ;;
	sip-uri-types) TAG="sip-uri-types-v$VERSION" ;;
	sip-uri-diagnostics) TAG="sip-uri-diagnostics-v$VERSION" ;;
	*)
		echo "Unknown crate: $CRATE (expected sip-uri, sip-uri-types or sip-uri-diagnostics)" >&2
		exit 1
		;;
	esac
	if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-rc\.[0-9]+)?$ ]]; then
		echo "Version must be X.Y.Z or X.Y.Z-rc.N (got: $VERSION)" >&2
		exit 1
	fi
	if [ ! -f "$CHANGELOG_FILE" ]; then
		echo "Changelog file not found: $CHANGELOG_FILE" >&2
		exit 1
	fi
	if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
		echo "Tag $TAG already exists." >&2
		exit 1
	fi

	CRATES+=("$CRATE")
	VERSIONS+=("$VERSION")
	TAGS+=("$TAG")
	CHANGELOGS+=("$CHANGELOG_FILE")
	SUBJECT="${SUBJECT:+$SUBJECT, }$CRATE v$VERSION"
done

# Whether crates.io has any release of $1, read from the sparse index.
published() {
	local name="$1" path status
	case ${#name} in
	1) path="1/$name" ;;
	2) path="2/$name" ;;
	3) path="3/${name:0:1}/$name" ;;
	*) path="${name:0:2}/${name:2:2}/$name" ;;
	esac
	status="$(curl -sS -o /dev/null -w '%{http_code}' "https://index.crates.io/$path")"
	case "$status" in
	200) return 0 ;;
	404) return 1 ;;
	*)
		echo "crates.io index returned HTTP $status for $name." >&2
		exit 1
		;;
	esac
}

# Replace the first line matching $2 in $1 with $3, failing when none matched.
replace_line() {
	local file="$1" pattern="$2" replacement="$3"
	if ! grep -qE "$pattern" "$file"; then
		echo "No line matching '$pattern' in $file." >&2
		exit 1
	fi
	sed -i -E "0,/$pattern/s//$replacement/" "$file"
}

for i in "${!CRATES[@]}"; do
	VERSION="${VERSIONS[$i]}"
	case "${CRATES[$i]}" in
	sip-uri)
		replace_line Cargo.toml '^version = "[^"]*"' "version = \"$VERSION\""
		;;
	sip-uri-types)
		replace_line sip-uri-types/Cargo.toml '^version = "[^"]*"' "version = \"$VERSION\""
		replace_line Cargo.toml '^sip-uri-types = \{ version = "[^"]*"' "sip-uri-types = { version = \"$VERSION\""
		;;
	sip-uri-diagnostics)
		replace_line sip-uri-diagnostics/Cargo.toml '^version = "[^"]*"' "version = \"$VERSION\""
		replace_line Cargo.toml '^sip-uri-diagnostics = \{ version = "[^"]*"' "sip-uri-diagnostics = { version = \"$VERSION\""
		;;
	esac
done

# semver-checks judges the change against the version being released, so it
# runs after the bump; a failure restores the manifests.
for crate in "${CRATES[@]}"; do
	if ! published "$crate"; then
		echo "semver-checks skipped for $crate: no release on crates.io to compare against."
	elif ! cargo semver-checks check-release -p "$crate"; then
		git checkout -- Cargo.toml sip-uri-types/Cargo.toml sip-uri-diagnostics/Cargo.toml
		echo "semver-checks failed for $crate; manifests restored." >&2
		exit 1
	fi
done

# Re-tagging a version the manifests already carry has nothing to commit.
if git diff --quiet -- Cargo.toml sip-uri-types/Cargo.toml sip-uri-diagnostics/Cargo.toml; then
	echo "Manifests already at the requested versions; no release commit."
else
	git add Cargo.toml sip-uri-types/Cargo.toml sip-uri-diagnostics/Cargo.toml
	git commit -m "release: $SUBJECT"
fi

git checkout --detach
if git symbolic-ref -q HEAD >/dev/null; then
	echo "Failed to detach HEAD; aborting before touching Cargo.lock." >&2
	exit 1
fi

cargo generate-lockfile
git add -f Cargo.lock
git commit -m "build: pin Cargo.lock for $SUBJECT"

for i in "${!TAGS[@]}"; do
	git tag -as "${TAGS[$i]}" -F "${CHANGELOGS[$i]}"
done

git switch "$BRANCH"

cat <<EOF
Tagged ${TAGS[*]} on a detached commit off $BRANCH.
Review:  git show ${TAGS[0]}
Push:    git push && wait for CI green on $BRANCH, then git push origin ${TAGS[*]}
Publish: scripts/release-publish.sh ${TAGS[*]}
EOF
