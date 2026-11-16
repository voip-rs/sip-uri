#!/bin/bash
# Compare HEAD's instruction counts against a base ref, usually the last
# release tag: build benches/count.rs for the base in a temporary worktree
# and for HEAD, run each workload under `perf stat -e instructions:u`, and
# print base, HEAD and the change. Exits 2 when a workload retires more than
# THRESHOLD percent more instructions, so a caller can stop and ask.
#
# Usage: scripts/bench-compare.sh <base-ref>
#
# Instructions, not time: a count barely moves with machine load, while a
# timing on a shared box drifts by more than most regressions. The base gets
# HEAD's benches/ copied in when it predates them, so both sides run the
# same workloads; a base the benches do not compile against fails here.

set -euo pipefail

base=${1:?usage: scripts/bench-compare.sh <base-ref>}
threshold=${THRESHOLD:-2}
root=$(git rev-parse --show-toplevel)
worktree="$root/worktrees/bench-base-$$"

cleanup() {
    git -C "$root" worktree remove --force "$worktree"
}
git -C "$root" worktree add --detach --quiet "$worktree" "$base"
trap cleanup EXIT

if ! test -f "$worktree/benches/count.rs"; then
    rm -rf "$worktree/benches"
    cp -r "$root/benches" "$worktree/"
    sed -n '/^\[\[bench\]\]$/,$p' "$root/Cargo.toml" >> "$worktree/Cargo.toml"
    grep -q '^criterion' "$worktree/Cargo.toml" \
        || sed -i "s/^\[dev-dependencies\]$/&\n$(grep '^criterion' "$root/Cargo.toml" | sed 's/[&/]/\\&/g')/" "$worktree/Cargo.toml"
fi

# Path of the count benchmark built in directory $1.
build() {
    (cd "$1" && cargo bench --bench count --no-run --message-format=json) \
        | jq -r 'select(.reason == "compiler-artifact" and .target.name == "count") | .executable'
}
base_bin=$(build "$worktree")
head_bin=$(build "$root")

# Instructions retired by workload $2 of binary $1, median of five runs.
count() {
    for _ in 1 2 3 4 5; do
        perf stat -x, -e instructions:u "$1" "$2" 2>&1 >/dev/null \
            | awk -F, '$3 ~ /^instructions/ { print $1 }'
    done | sort -n | sed -n 3p
}

regressed=0
printf '%-18s %16s %16s %8s\n' workload "$base" HEAD change
for workload in $("$head_bin"); do
    a=$(count "$base_bin" "$workload")
    b=$(count "$head_bin" "$workload")
    change=$(awk -v a="$a" -v b="$b" 'BEGIN { printf "%+.2f", (b - a) * 100 / a }')
    flag=""
    if awk -v c="$change" -v t="$threshold" 'BEGIN { exit !(c > t) }'; then
        flag=" REGRESSED"
        regressed=1
    fi
    printf '%-18s %16s %16s %7s%%%s\n' "$workload" "$a" "$b" "$change" "$flag"
done
if test "$regressed" = 1; then
    echo "instruction count regressed by more than ${threshold}% against $base" >&2
    exit 2
fi
