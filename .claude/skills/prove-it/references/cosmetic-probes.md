# Cosmetic probes — L4

Every probe is a **delta**: run at HEAD and at base, report only what the diff added. Every probe
has an explicit word boundary and a kill list. Use `git grep` / `git diff`, never `grep -r` or
`find` (worktrees and `target/` multiply the answer). Under `-E`, write `[[:space:]]`, not `\s`,
and a bare `|` for alternation.

**Pathspec trap measured in this repo:** an exclusion like `':!**/*test*'` also drops
`sdks/go/latest_state.go`, because `latest` contains `test`. Exclude test paths with
`':!**/*_test.go' ':!**/tests/**' ':!**/test/**' ':!**/*_tests.rs'`, never a bare `*test*`.

## Universal probes

| Id | Probe | The kill that must be ruled out first |
|---|---|---|
| P1 | A listener/handler with nothing producing its event | the name is built by string interpolation, held in a constant, or produced from another language (an event type emitted by Core and consumed by QS or the control plane is the common case here) |
| P2 | An enum variant or case constructed nowhere | a serde/Jason/encoding/json derive is an invisible producer: read the derive line and grep the **wire string** (`rename_all`, `#[serde(rename = …)]`, `json:"…"` tag), not the identifier |
| P3 | A test that re-declares its subject | none — a test declaring a function that shares a name with an export of the module it names is testing its own copy |
| P4 | A test green on absent input | an env-var early return, a `#[ignore = "…"]` whose reason names an external resource (convention) rather than an unlanded fix (finding), a filter matching nothing, a mock so complete the assertion can only be true; **and here specifically**: a `#![cfg(feature = …)]` suite whose feature is off in CI (guard-ratchets G2) |
| P5 | An unwired gate — **state the tier** (coverage ledger § P5) | it is wired into `ci.yml` and the job id strings in the `if:`, the `needs:` list and the `quality-gate` `check_job` line all match **exactly** |
| P6 | A gate whose own pattern cannot match | test every regex the diff adds against a real line from a file inside that gate's own roots |
| P7 | An unreachable module or component | a dynamic import, a mock path string, a Next.js file-system route under `apps/web/src/app` or `apps/admin/src/app` (reached by its path, not an import), a Rust module reached only under a `#[cfg(feature)]`; exclude `target/`, `.next/`, `_build/`, `deps/` |
| P8 | Config theatre — a flag, setting or env var nothing reads | read by a consumer outside this repo (a Fly secret consumed by another app, a build arg in a `fly.toml`); an env var referenced only in a runbook is documentation, not configuration |
| P9 | Scaffold on the feature's own path | off the claim's path it is ordinary; on it, the claim is refuted |
| P10 | Doc-only completion | pick the doc's single most concrete claim and grep the **implementation**, not the name |

Probe commands for P2 and P8 in this repo:

```
# P2: a new Rust enum variant; then grep its wire string across apps/ and sdks/
git diff <base>..HEAD -U0 -- '*.rs' | grep -E '^\+[[:space:]]+[A-Z][A-Za-z0-9]*([[:space:]]*[({,]|$)'

# P8: a new env var read; then grep every place that sets it (fly.toml, Dockerfile, workflows, runbooks)
git diff <base>..HEAD -U0 | grep -oE '(env::var|std::env::var|System\.get_env|os\.Getenv|process\.env\.)[(" ]*[A-Z][A-Z0-9_]+'
```

## Repo probes — from this repo's own incidents and rules

### R1 · A dependency bump that only touched manifests

> A sha2 0.10 → 0.11 and aes-gcm 0.10 → 0.11 bump changed only `Cargo.toml` and `Cargo.lock`; Core
> stopped compiling on `main` with 22 errors in files neither commit touched, and both were
> reverted (`56e70b5b`). Dependabot labels a 0.x minor as semver-minor; it is a breaking change.

```
git diff --name-only <base>..HEAD
git diff <base>..HEAD -U0 -- '*Cargo.toml' | grep -E '^[+-][a-z0-9_-]+ *= *("|\{ *version *= *")0\.'
```

Check: when only manifests and lock files changed and a `0.x` requirement moved its minor, the
claim "bump X" is `REFUTED` unless the unit that depends on it was compiled on this head. Ask for
the build's output, or run the scoped check: `cargo check --locked --manifest-path <unit>/Cargo.toml`
(one build on the machine at a time).

### R2 · A tracker id in source

Rule (global `git-discipline`, `comments`): never a task-tracker id in source, comments included.

```
git diff <base>..HEAD -U0 -- '*.rs' '*.ex' '*.exs' '*.go' '*.ts' '*.tsx' '*.py' | grep -E '^\+.*(//|#|/\*|\*|@moduledoc|""").*(^|[^A-Za-z0-9_-])t-[0-9a-f]{6}([^0-9a-f]|$)'
```

Standing hits exist at base; only additions count. Kill: chronis's own id generator documents its
format (`apps/chronis/src/infrastructure/id.rs`), and a test fixture string that **is** a task id.

### R3 · A comment that narrates the fix

Rule (global `comments`): no history of the fix, no quote of what it replaced, no record of what
was not done. Judged, not grepped. Read every `+` comment line the diff adds and ask the three
tests: does it say something the code cannot; would a reader who never saw the old version need
it; is it a fact rather than a story. A failure is `sand`, never `rock`.

### R4 · Core responses unwrapped

Rule (repo `CLAUDE.md` § Core API): Core wraps list responses in maps (`{"events": [...]}`,
`{"projections": [...]}`); `RustCoreClient` unwraps them. A new Core list endpoint returning a bare
JSON array breaks that contract. Judged: for each new route (placement-map P-L1i), read the
handler's return type.
