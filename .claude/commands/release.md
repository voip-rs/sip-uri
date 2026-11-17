Perform a release of sip-uri, sip-uri-types and/or sip-uri-diagnostics.

Optional override: $ARGUMENTS (format: `<crate> X.Y.Z[-rc.N]` pairs). If provided, use those versions.

## Crates and tags

| Crate | Directory | Tag |
|---|---|---|
| sip-uri-diagnostics | `sip-uri-diagnostics/` | `sip-uri-diagnostics-vX.Y.Z[-rc.N]` |
| sip-uri-types | `sip-uri-types/` | `sip-uri-types-vX.Y.Z[-rc.N]` |
| sip-uri | repo root | `vX.Y.Z[-rc.N]` |

sip-uri depends on sip-uri-types and sip-uri-diagnostics through `path` + `version` requirements. Releasing either bumps sip-uri's requirement on it too, to the released minor (`"1"`, `"1.1"`) or a release candidate in full, so sip-uri needs a release whenever its Cargo.toml changed. A sip-uri change that uses a code or item added in a sibling's minor raises that requirement in the same commit, before any release. sip-uri-types and sip-uri-diagnostics do not depend on each other.

## Version determination

1. Find each crate's last release tag (`git tag --sort=-v:refname -l 'v*' | head -1`, `git tag --sort=-v:refname -l 'sip-uri-types-v*' | head -1`, `git tag --sort=-v:refname -l 'sip-uri-diagnostics-v*' | head -1`).
2. Examine commits since that tag touching the crate (`git log --oneline <last-tag>..HEAD -- sip-uri-types/` for the data crate, `-- sip-uri-diagnostics/` for the vocabulary crate; everything else for sip-uri) to classify the release:
   - sip-uri (0.x: the minor is the breaking axis): **patch** (0.Y.z+1) for bug fixes, additive API, dependency bumps, build changes, docs; **breaking** (0.Y+1.0) for changed/removed public items or incompatible behavior.
   - sip-uri-types (1.x): **patch** for fixes and docs, **minor** for additive API, **major** for anything breaking. A change to a value's identity (canonical form, equality, Display) is breaking.
   - sip-uri-diagnostics (1.x, only grows): **patch** for fixes and docs, **minor** for a new warning code, component or other additive API, **major** for anything breaking, which includes removing or renaming a code or component or changing an `as_str` name.
   - Stop and confirm before any breaking release.
3. A version still in release candidates stays one: bump `-rc.N` until told to cut the final version.

## Steps

1. Start the benchmark comparison against sip-uri's last release tag in the background (Bash `run_in_background`), and go on with the next steps while it runs:

```sh
scripts/bench-compare.sh <last-sip-uri-tag>
```

   It builds the benchmark workloads for the tag in a temporary worktree and for HEAD, and compares the instructions each retires under `perf stat`; a count barely moves with machine load, so the other steps may run meanwhile. It must have finished before step 6 pushes any tag: exit 0 lets the release go on; exit 2 is a workload more than 2% costlier, so stop, show me its table, and ask whether the regression is acceptable; any other failure, stop and report.

2. Pre-release checks — stop and report on any failure:

```sh
scripts/release-check.sh
```

   Runs fmt, clippy, docs and tests across the workspace with all features, and `cargo publish --workspace --dry-run`, which verifies sip-uri against the sip-uri-types and sip-uri-diagnostics being released.

3. Draft one changelog per crate from its commits since its last tag and write each to `scratch/changelog-<tag>.txt` (gitignored; not part of the published package).

   **Rules:**
   - Group under: `New features:`, `Bug fixes:`, `Build:`, `Refactoring:` — omit empty sections.
   - Describe user-visible behavior, not implementation details.
   - Merge related commits for the same feature into one bullet.
   - No git hashes, no raw commit subjects, no co-author lines.
   - A breaking release opens, under its tag line, with one sentence pointing at its migration guide on docs.rs (`https://docs.rs/sip-uri/<version>/sip_uri/migrating_from_<old>/index.html`), and that guide covers every break the changelog lists.

   File format (becomes the tag annotation verbatim):
   ```
   <tag>

   New features:
   - what changed

   Bug fixes:
   - what was fixed

   Build:
   - what changed
   ```

4. Bump, commit, and tag, sip-uri-diagnostics, then sip-uri-types, then sip-uri, listing only the crates being released:

```sh
scripts/release-tag.sh sip-uri-diagnostics X.Y.Z scratch/changelog-sip-uri-diagnostics-vX.Y.Z.txt sip-uri-types X.Y.Z scratch/changelog-sip-uri-types-vX.Y.Z.txt sip-uri X.Y.Z scratch/changelog-vX.Y.Z.txt
```

   Bumps each crate's `Cargo.toml` (and sip-uri's requirement on a bumped sip-uri-types or sip-uri-diagnostics), runs `cargo semver-checks` per crate against the new version (skipped for a crate with no release on crates.io yet; a failure restores the manifests and stops), commits `release: …`, detaches HEAD, pins `Cargo.lock` on that detached commit (`build: pin Cargo.lock for …`), signs one tag per crate on it from its changelog file, and returns to the branch. Refuses to run on a dirty tree, off master, or if a tag already exists. Nothing is pushed yet.

5. Push master, wait for CI green:

```sh
git push
gh run watch "$(gh run list --workflow=ci.yml -b master -L1 --json databaseId --jq '.[0].databaseId')" --exit-status
```

   No run within a couple of minutes: check the `Actions` component at `https://www.githubstatus.com/api/v2/components.json` — during an outage no run is created and missed events are never backfilled. Stop and report.

   Red: fix on master, delete the local tags (`git tag -d <tag>…`), rebuild them with `scripts/release-tag.sh` onto the new head, rerun step 1 on it, restart this step.

6. Push the tags:

```sh
git push origin <tag>...
```

   A tag is IMMUTABLE once pushed — never retag. Wrong? Make a new patch release (or the next `-rc.N`).

7. Publish:

```sh
scripts/release-publish.sh <tag>...
```

   Checks out the tags' commit, checks each tag against its crate's manifest version, runs one `cargo publish --dry-run` over the tagged crates, then publishes sip-uri-diagnostics, sip-uri-types and sip-uri in that order, and returns to the branch.

8. Report the tags, the changelogs, the CI run that gated the publish, and the crates.io versions (`curl https://index.crates.io/si/p-/sip-uri-diagnostics`, `curl https://index.crates.io/si/p-/sip-uri-types`, `curl https://index.crates.io/si/p-/sip-uri`).

## Important

- **Never publish a commit CI has not run on.** The tags' pin commit differs from the CI-green master tip only by `Cargo.lock`. If anything else changed after the checks — a rebase, a hand-resolved conflict — the earlier green run does not cover it. Re-run the checks and go back to step 5.
- **Ask before publishing when anything deviated from these steps.** An outage, a rebase, a skipped step, a red-then-fixed run: report the state and let me decide.
- **Cargo.lock never reaches master** — library crates, stays gitignored there. It exists only on the tags' own commit, so a release build is reproducible.
- **A sip-uri release candidate names a sip-uri-types or sip-uri-diagnostics release candidate by its full version** (`"1.0.0-rc.1"`), since a caret requirement only matches prereleases when it carries one itself.
