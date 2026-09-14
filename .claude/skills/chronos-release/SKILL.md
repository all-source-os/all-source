---
name: chronos-release
description: Release a new version of the AllSource Chronos monorepo. Bumps versions across all services (Rust, Go, Elixir), runs full CI to green, creates a single squashed commit with an annotated immutable tag. Use when the user says "release", "new version", "bump version", "tag a release", "cut a release", or "make a release". Argument is the version number (e.g., "0.10.4") or "patch"/"minor"/"major" for auto-increment.
---

# Chronos Release Skill

Cut a release for the AllSource Chronos monorepo. Produces exactly one commit and one immutable annotated tag.

## Scope — full monorepo vs. SDK-only

**This skill is for full monorepo releases** that bump Core, Query Service, and all SDKs together. Those use the unscoped tag `v<VERSION>` and run the full `make ci` gate (quality-rust + quality-go + quality-elixir-full).

**For SDK-only releases** (bumping only `sdks/<lang>/` without touching apps/), do NOT use this skill. Instead:

- Tag format: `sdk-<lang>-v<VERSION>` — e.g. `sdk-rust-v0.19.0`. Never `v<VERSION>`.
- Gate: only the relevant language gate (e.g. `make quality-rust` for Rust SDK) plus `cargo publish --dry-run` for published crates.
- Commit: `sdk(<lang>): release v<VERSION> — <headline>`.
- Only the SDK's `Cargo.toml`/equivalent + its `CHANGELOG.md` should change.

The scoping keeps per-SDK version numbers from colliding with monorepo version numbers (e.g. `sdk-rust-v0.19.0` can ship while Core is still at `0.18.x`).

## Immutable Tags Policy

**NEVER** move, delete, or re-create an existing tag. If a release has issues after tagging, bump the version and cut a new release (e.g., v0.10.4 instead of re-tagging v0.10.3).

Before starting, verify the requested tag does not already exist:
```bash
git tag -l "v<VERSION>"
# or for SDK-only:
git tag -l "sdk-<lang>-v<VERSION>"
```
If it exists, abort and tell the user to choose a higher version.

## Procedure

### 1. Determine version

If the user provides a semver string, use it. If they say "patch", "minor", or "major", read the current version and increment:
```bash
grep 'version = ' apps/core/Cargo.toml | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'
```

### 2. Check preconditions

- Working tree must be clean (`git status --porcelain` empty) OR the user explicitly wants to include staged changes
- Current branch should be `main` (warn if not)
- Tag `v<VERSION>` must not exist

### 3. Bump versions

```bash
make set-version VERSION=<VERSION>
```

This updates: `Cargo.toml`, `main.go`, `tracing.go`, `mix.exs` (x2), K8s manifests, `README.md`, `apps/prime-mcp/Cargo.toml` (version + allsource-core dep).

After running, also update `Cargo.lock`:
```bash
cd apps/core && cargo update --workspace
```

Check for any other version references that `set-version` might miss:
- `apps/control-plane/docs/openapi.yaml` (info.version field)
- `apps/core/README.md`, `apps/mcp-server-elixir/README.md` (version badges/text)
- `docs/` files referencing the old version

### 4. Run CI to green

**HARD GATE: Do NOT proceed to step 5 until ALL three gates pass. No exceptions.**

A tag is immutable. Once pushed, a broken release means burning another version number. The cost of re-running CI is minutes; the cost of a bad tag is permanent version pollution.

**Pre-fix common issues BEFORE the first CI attempt** to minimize iterations:
```bash
# Always run these first:
cargo +nightly fmt --all
cargo +nightly sort --workspace
cd apps/mcp-server-elixir && mix format && cd ../query-service && mix format && cd ../..
```

**Run the three quality gates in PARALLEL:**

```bash
# Launch all three in parallel (use run_in_background for each)
make quality-rust 2>&1 | tail -5      # ~3-4 min (clippy + test + doc)
make quality-go 2>&1 | tail -5        # ~30 sec
make quality-elixir-full 2>&1 | tail -10  # ~4-5 min (dialyzer + tests)
```

Wait for ALL THREE to complete and verify EACH returned exit code 0.

**If any gate fails:**
1. Fix the issue
2. Re-run the failing gate(s)
3. **Verify ALL gates pass before proceeding** — do not assume passing gates still pass after fixes

**Known pre-existing failures to distinguish from new issues:**
- Go: `TestAdminBillingGetDunning_WithPastDue` is flaky — passes on retry
- Go: coverage tool version mismatch (go1.26.1 vs go1.26.0) — cosmetic, tests still pass
- Rust `--all-targets`: testcontainers dep mismatch — `--lib` tests pass fine
- Elixir MCP: ConversationContext GenServer not started in test env — 2 known failures

**For targeted re-checks after fixes**, only re-run the affected gate:
- Rust fix → `make quality-rust`
- Elixir fix → `make quality-elixir-full`
- Go fix → `make quality-go`

### 5. Commit (single squashed commit)

**NEVER `git add -A` here.** Other agent sessions work this repo concurrently, and
a release runs long enough for one of them to leave edits in the tree mid-run. On
2026-09-12 the v0.24.0 release did exactly that: `git add -A` swept another
session's in-flight manifest edits into the release commit. That release then
shipped a Core version bump alongside a half-applied dependency change, and
`allsource-mcp` could not resolve on `main` until the next commit. A release
commit that silently carries someone else's unfinished work is how a broken
change ships under a version tag.

First, read the tree and confirm every dirty file is yours:

```bash
git status --short
```

Anything you did not change as part of this release — stop. Ask whose it is
before going further; do not stage it, and do not revert it either.

Then stage the release's files **by explicit path** and verify the index before
committing:

```bash
git add <each file the version bump touched>
git diff --cached --stat   # read it: this must be exactly the release's files
git commit -m "$(cat <<'EOF'
release: v<VERSION> — <brief description of what changed>

<bullet list of key changes>
EOF
)"
```

**Critical**: There must be exactly one new commit for the release. If you need multiple rounds of CI fixes, do NOT commit between rounds — only commit once everything is green.

### 6. Tag (immutable)

Create an annotated tag:
```bash
git tag -a v<VERSION> -m "v<VERSION> — <brief description>"
```

### 7. Verify

```bash
git log --oneline -3
git show v<VERSION> --oneline --no-patch
```

Confirm: single new commit, tag points to it.

### 8. Report — DO NOT PUSH

**CRITICAL: Do NOT push automatically. Do NOT offer to push. Do NOT push even if the user says "release it" or "push it" in the same message as the release request.**

Tell the user:
- Version: v<VERSION>
- Commit: <hash>
- Tag: v<VERSION>
- CI: green (list which gates passed, note any pre-existing failures)
- **The user must manually run `git push && git push --tags` when ready**

The user needs a chance to review the commit, verify the tag, and decide when to push. A tag cannot be undone.

### 9. Publish Rust crates (after push)

After the user pushes, publish Rust crates to crates.io in dependency order:

```bash
# 1. Core (must be first — other crates depend on it)
cd apps/core && cargo publish

# 2. Prime MCP Server (depends on allsource-core)
cd apps/prime-mcp && cargo publish

# 3. Rust SDK
cd sdks/rust && cargo publish
```

**Wait ~60 seconds between publishes** for crates.io index to update.

**Verify** `cargo install allsource-prime` works after publishing.

**DO NOT publish** TypeScript, Python, or Go SDKs — they're obtained through GitHub, not package registries.

### 10. Batch fixes — do NOT release per-fix

**If multiple issues need fixing, batch them into ONE release.** Do not cut a separate release for each fix. The version number is a scarce resource — every botched release wastes one permanently.

Checklist before tagging:
- [ ] All planned fixes are implemented and tested
- [ ] ALL three CI gates pass (not just the ones related to your changes)
- [ ] `mix format` has been run on ALL Elixir apps (not just the one you edited)
- [ ] `cargo +nightly fmt --all` has been run
- [ ] No unrelated dirty files will be swept into the commit (`git status`)
- [ ] Dockerfiles have been verified against current workspace members

## Common CI Fix Patterns

| Tool | Common Issue | Fix |
|------|-------------|-----|
| cargo clippy | collapsible_if | Use let-chain syntax: `if cond1 && cond2 { }` |
| cargo clippy | new_without_default | Add `impl Default for T { fn default() -> Self { Self::new() } }` |
| cargo clippy | redundant_closure | `.map(Foo::bar)` instead of `.map(\|x\| Foo::bar(x))` |
| cargo sort | unsorted deps | `cargo +nightly sort` |
| cargo fmt | formatting | `cargo +nightly fmt` |
| rustdoc | broken_intra_doc_links | Wrap brackets in backticks |
| golangci-lint | goconst | Extract repeated strings to constants |
| golangci-lint | gocritic ifElseChain | Convert to switch statement |
| mix credo | alias ordering | Alphabetize alias statements |
| mix credo | cyclomatic complexity | Refactor or add `# credo:disable-for-next-line` |
| mix format | formatting | `mix format` in each Elixir app |
| mix dialyzer | pattern_match warnings | Add entries to `.dialyzer_ignore.exs` |
