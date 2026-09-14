# crate-version-check

CI gate. Every intra-repo path dependency's **version requirement** must match
the version the **sibling crate actually declares**.

```bash
cargo run --manifest-path tooling/crate-version-check/Cargo.toml
```

Exit 0 when consistent, 1 with the offending manifests named.

## The incident

A dependency on a sibling crate carries two facts:

```toml
allsource-core = { version = "0.23.1", path = "../../apps/core" }
```

`path` decides what cargo compiles locally. `version` decides what the registry
serves once `cargo publish` strips the path. When they disagree, the crate does
not resolve at all:

```
error: failed to select a version for the requirement `allsource-core = "^0.23.1"`
candidate versions found which didn't match: 0.24.0
```

On 2026-09-12 the `v0.24.0` release bumped `apps/core` to 0.24.0 across sixteen
files and left `tooling/allsource-mcp` requiring `^0.23.1`. That crate could not
build on `main` until the following commit. Nothing failed: a `cargo publish`
attempt surfaced it, and only because publish verification builds against the
*registry* copy. With `--no-verify` the broken requirement would now be permanent
on crates.io, where a version can be yanked but never corrected.

`scripts/check-versions.sh` does not cover this. It compares **toolchain**
versions — Rust, Go, Node across Dockerfiles, CI and manifests. Nothing compared
one crate's requirement against a sibling's actual version.

Seven manifests in this repo declare `allsource-core`. The defect has that many
doors; this closes all of them.

## What it checks

- Inline deps (`foo = { path = "..", version = ".." }`), including ones wrapping
  across lines.
- Sub-table deps (`[dependencies.foo]`).
- `dependencies`, `dev-dependencies`, `build-dependencies`, and their
  `target.*` variants.

Requirements are matched with cargo's caret rules, so `^0.23.1` accepts 0.23.2
but not 0.24.0.

A dependency with a `path` and **no** `version` is not checked — there is no
registry claim to contradict. A requirement using any other operator (`=`, `~`,
`>=`, `*`, a comma range) is printed as a `note:` and not judged, so an unchecked
dependency stays visible rather than being silently assumed fine.

## Verifying it can fail

A gate that only ever passes is indistinguishable from one that does nothing.
Point it at a throwaway tree — a directory containing `.git/` plus two crates
where one declares a stale requirement on the other — and confirm it exits 1 and
names both the manifest and the fix.
