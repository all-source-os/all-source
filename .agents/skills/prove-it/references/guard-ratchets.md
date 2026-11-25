# Guard ratchets — L3

Nothing here can block a merge (see the coverage ledger), so a gate loosened in CI is the loss of
the only check there is. **Direction lives in the command**: pair `-`/`+` tuples by key and compare
values. A grep matching `^[+-]` alone matches both directions identically.

Values below were read at `main@b6614086`. Re-read them at base every run; never assert them.

## G1 · Workflow invocation flags

```
git diff -M <base>..HEAD -- .github/workflows | grep -E '^[+-][^#]*(--features|--all-targets|--all-features|--no-default-features|--tests|--lib|-D warnings|--locked|--strict|--warnings-as-errors|--check|continue-on-error|\|\| true|if: false|timeout|paths:|branches:|needs:|-run |--include)'
```

The `[^#]*` keeps comment prose out. For each key on both a `-` and a `+` line, report only where
the value loosened. Sign per kind:

| Change | Direction |
|---|---|
| a feature dropped from `Run integration tests` `--features …` | **loosened**: suites gated on it compile to zero tests ("ok. 0 passed") or are skipped silently |
| `--all-targets` → narrower, `-D warnings` dropped, `--locked` dropped | loosened |
| `continue-on-error: true`, `or-true` (a trailing double-pipe `true`), `if: false` added | loosened: the step can no longer fail anything |
| a path removed from a `dorny/paths-filter` key, or from a `paths:` trigger | loosened: changes under it stop running the job |
| a path **added** to a filter key or `paths:` | tightened |
| a job removed from `quality-gate.needs`, or its `check_job` line removed | loosened |
| `cargo +nightly` added to a step | loosened: it overrides `rust-toolchain.toml` (`channel = "nightly-2026-07-16"`) and the gate can go red without the code changing (comment above `Install Rust toolchain` in `ci.yml`) |

## G2 · The integration-test feature list must cover every gated suite

The list in `ci.yml` step `Run integration tests` is the coverage of `apps/core/tests/`. Two ways
a diff silently removes coverage:

1. it adds a test file gated on a feature the list lacks;
2. it removes a feature from the list.

```
git diff <base>..HEAD -U0 -- 'apps/core/tests/*.rs' | grep -E '^\+#!\[cfg\('
git diff <base>..HEAD -U0 -- apps/core/Cargo.toml | grep -E '^\+required-features'
```

For each feature named, check it is in the list or implied by one in it
(`git grep -nE '^<feature> *=' -- apps/core/Cargo.toml` shows the implication; `default` is
`["server", "community"]`). A new gated suite whose feature is not covered is `rock`
(`SILENT-RISK`): the suite will report green without running.

Incident: before `36f84dd9` nothing in `apps/core/tests/` ran; after it, `da458672` measured 5
integration suites executing zero tests and 10 more never built, because their features were off.
Turning them on exposed a sync defect in a suite that had never executed.

## G3 · Filter key coverage for a new unit

A diff that adds a unit (a new manifest under `apps/`, `crates/`, `sdks/`, `tooling/`) must add it
to a `dorny/paths-filter` key and a job, or the unit joins § Nowhere in the coverage ledger.

```
git diff --name-status <base>..HEAD | grep -E '^A.*(Cargo\.toml|package\.json|go\.mod|mix\.exs|pyproject\.toml)$'
git diff <base>..HEAD -- .github/workflows/ci.yml
```

New unit, no workflow change → `sand` routed to the author: "which job runs this unit's tests?"

## G4 · Lint and dependency policy files

| File | Key | Value at census | Closed and empty? |
|---|---|---|---|
| `apps/core/deny.toml` | `[advisories] ignore` | `[]` (only a commented example) | **yes** — a first `RUSTSEC-` entry is the strongest single signal here |
| `apps/core/deny.toml` | `[bans] skip`, `skip-tree`, `allow` | `[]` each | **yes** |
| `apps/core/deny.toml` | `[sources] allow-git` | `[]` | **yes** |
| `apps/core/deny.toml` | `[bans] multiple-versions` | `"warn"` | — |
| `apps/control-plane/.golangci.yml` | `linters.enable`, `disable`, `disabled-checks`, `excludes` | read at base | — |
| `apps/query-service/.dialyzer_ignore.exs` | filter list | read at base | — |
| `apps/query-service/.credo.exs`, `apps/mcp-server-elixir/.credo.exs` | checks | read at base | — |
| `biome.json` | `files.includes` negations | `!**/node_modules`, `!**/dist`, `!**/build`, `!**/.next`, `!**/target`, `!**/*.min.js`, `!**/*.d.ts`, `!**/.beads`, `!**/deps`, `!**/_build` | — (biome runs in no workflow) |
| `rust-toolchain.toml` | `channel` | `"nightly-2026-07-16"` | — |
| `ci.yml` `actionlint` | `-ignore` list | `SC2086`, `SC2129`, `SC2193` | — |
| `perf-bench.yml` iai compare | max percent | `3` | — |

Which configs a check actually reads: `git grep -n 'cargo deny' -- .github` finds two sites.
`bandwidth-alerts.yml` runs `cargo deny check` in `tooling/monitor-stack`, so it reads that
crate's config. `security.yml` step `Check Rust licenses` runs `cargo deny check licenses || true`
with `working-directory: apps/core`: it reads only the `[licenses]` section of
`apps/core/deny.toml`, cannot fail (`|| true`), and runs on push to `main` and on schedule, never
on a PR. `[advisories]`, `[bans]` and `[sources]` in `apps/core/deny.toml` are read by no workflow.
A loosened `apps/core/deny.toml` is therefore **tier 3**: say so rather than calling it gated.

## G5 · Inline overrides — a guard by another name

| Marker | Gate it overrides |
|---|---|
| `ISOLATION_OK: <reason>` | `Tenant Isolation Gate` check 1 (QS PubSub topics) |
| `CORE_PROJECTION_OK: <reason>` | `Tenant Isolation Gate` check 2 (per-tenant compute in Core); no current sites, so any addition is the first |
| `#[allow(` / `#![allow(` | clippy `-D warnings` |
| `//nolint` | golangci-lint |
| `credo:disable` | credo `--strict` |
| `#[ignore` | the test runner |

```
git diff <base>..HEAD -U0 -- apps/query-service/lib | grep -E '^\+.*ISOLATION_OK'
git diff <base>..HEAD -U0 -- apps/core/src | grep -E '^\+.*CORE_PROJECTION_OK'
git diff <base>..HEAD -U0 -- '*.rs' | grep -E '^\+[[:space:]]*#!?\[(allow|ignore)'
git diff <base>..HEAD -U0 -- '*.go' | grep -E '^\+.*//nolint'
git diff <base>..HEAD -U0 -- '*.ex' '*.exs' | grep -E '^\+.*credo:disable'
```

Each added marker on the feature's own path: read the reason. An external resource (a model fetch,
a live service, a large synthetic archive) is convention. An unlanded fix is a finding.

## G6 · A moved file leaves a named-path list

```
git diff -M --name-status <base>..HEAD | grep -E '^R'
```

For each rename, check whether the **old** path is named in a by-name list and the new one is not:
`dorny/paths-filter` keys and `paths:` triggers in `.github/workflows/`; `SCAN_DIR` /
`CORE_SCAN_DIR` in `tooling/tenant-isolation-check/src/main.rs`; `[workspace] members` / `exclude`
in root `Cargo.toml`; `[[test]] path =` in `apps/core/Cargo.toml`. A rename out from under one of
these is `rock`: the check scans nothing and exits 0.

## G7 · Silent success

```
git diff -M <base>..HEAD -U3 | grep -nE '^\+.*(unwrap_or_default\(\)|\.ok\(\);|let _ = |_ = err|rescue _|catch \(\) \{\}|except: *pass)'
```

A finding only when the same hunk has a `-` line that propagated the error (`?`, `return Err`,
`{:error, _}`). Kill: a code move, where the `+` and `-` lines are the same statements at a new
address. Extraction into a `store_*.rs` sibling is the sanctioned way to split a large module.
