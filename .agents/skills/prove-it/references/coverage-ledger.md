# Coverage ledger

Three questions per check: **does it run** (trigger), **does it look at the changed path**
(scan root), **can it stop a merge** (authority). Authority is the same for every row: **none**.
`gh api repos/all-source-os/all-source/branches/main/protection` → `404 Branch not protected`;
`gh api repos/all-source-os/all-source/rulesets` → `[]`. Repo is `PUBLIC` (`gh repo view`).

Census commands: `git grep -nE '^on:|…' -- .github/workflows`, and a read of
`.github/workflows/ci.yml` jobs `changes`, `*-quality`, `tenant-isolation`, `quality-gate`.
Re-grep every symbol here before a finding rests on it (discipline rule 7).

## Buckets

| Bucket | Meaning | Effect |
|---|---|---|
| **FIRES, covers it** | selected, and its scan root covers the changed paths | **DELEGATED** — the panel does not re-derive anything in its remit |
| **FIRES, partially blind** | selected, but its scan root is narrower than its trigger | **REVIEWED.** A firing check never suppresses a hole inside its own remit |
| **EXISTS, did not fire** | declared, remit touched, trigger missed it | hand-check: one command, exit-0 pass |
| **Nowhere, ever** | orphaned, or ungated | the panel's own beat |

## The trigger: `ci.yml` job `changes` (dorny/paths-filter)

Every `ci.yml` quality job is gated on one filter key **or** `workflows` (any change under
`.github/workflows/**` runs every job). On a push whose `before` SHA is unreachable, the
`Verify diff base is reachable` step sets `force=true` and every key is true.

| Filter key | Paths |
|---|---|
| `rust` | `apps/core/**`, `Cargo.lock`, `tooling/tenant-isolation-check/**` |
| `go` | `apps/control-plane/**` |
| `web` | `apps/web/**`, `packages/ui/**`, `package.json`, `bun.lock` |
| `elixir` | `apps/query-service/**` |
| `mcp` | `apps/mcp-server-elixir/**` |
| `prime` | `apps/prime-mcp/**`, `apps/core/src/prime/**`, `tooling/recall-bench/**` |
| `better-auth` | `crates/better-auth-allsource/**` |
| `sdks` | `sdks/**` |
| `allsource-mcp` | `tooling/allsource-mcp/**`, `apps/core/**` |
| `workflows` | `.github/workflows/**` |

**Compare id strings exactly.** The `quality-gate` job repeats each job's `if:` expression by hand
in its `check_job` calls. A diff that edits one copy and not the other makes a required job
"excused" or a skipped job "required". Diff both.

## Rows — `ci.yml` (push to `main`/`develop`, `pull_request` to `main`/`develop`)

| Check (job `name:`) | Trigger | Scan root / what runs | Bucket for a change under its trigger |
|---|---|---|---|
| `Workflow Syntax` | always | `actionlint -ignore 'SC2086' -ignore 'SC2129' -ignore 'SC2193'` over workflows | 1 for workflow syntax |
| `Workflow Validation (act)` | always | `act push --dryrun -W .github/workflows/ci.yml --job workflow-lint` — **one job of one workflow** | 2: other workflows and jobs are only actionlinted |
| `Version Consistency` | always | `cargo run --manifest-path tooling/crate-version-check/Cargo.toml` | 1 for intra-repo version requirements |
| `Detect Changes` | always | the filter table above | — |
| `Web Brand Color` | `web` | `bun --cwd apps/web test src/__tests__/brand-background.test.ts` — **one test file** | **2**: every other change under `apps/web/**`, `packages/ui/**` fires this and nothing else: `git grep -nE 'apps/(web|admin|auth)' -- .github/workflows` returns only the filter line and this step. No type-check, no lint, no other test, no build in any workflow |
| `Rust Quality Gates` | `rust` | in `apps/core`: `cargo fmt --check`, `cargo sort --check`, clippy `--all-targets --all-features -D warnings`, lib tests (all-features and `--features community`), `cargo test --tests --features embedded-phase2,embedded-sync,embedded-replicant,embedded-streaming,embedded-projections,embedded-toon,prime-recall,replication,multi-tenant,cold-tier-s3`, release build, `cargo doc -D warnings` | 1 for `apps/core`, **except**: an integration test gated on a feature outside that list compiles to an empty binary or is skipped (see § Nowhere: `heap_profile`). `Cargo.lock` triggers it but only `apps/core` is compiled: a lock change for another workspace member (`sdks/rust`, `tooling/performance`) is **2** |
| `Tenant Isolation Gate` | `rust` or `elixir` | `SCAN_DIR = "apps/query-service/lib"`, `CORE_SCAN_DIR = "apps/core/src"` in `tooling/tenant-isolation-check/src/main.rs` | 1 for those two roots. **Not in `quality-gate.needs`** (`git grep -n -A16 '^  quality-gate:' -- .github/workflows/ci.yml`), so a red run leaves `Quality Gate` green |
| `Prime MCP Quality Gates` | `prime` | in `apps/prime-mcp`: fmt, clippy `--all-targets --no-default-features`, `cargo test --bins --tests --no-default-features`, profiling check, release build, two embedder runs; `cargo build` in `tooling/recall-bench` | 1 for `apps/prime-mcp`. `crates/hound-extract` is a path dependency compiled by this job, but no filter key lists `crates/hound-extract/**`: a change there alone is **3** |
| `better-auth-allsource Quality Gates` | `better-auth` | `crates/better-auth-allsource`: fmt, clippy `--all-targets`, `cargo test --locked` | 1 |
| `SDK Quality Gates` | `sdks` | Rust SDK (default, `es256-assertion`, `gzip`), Go SDK (`gofmt`, vet, `go test -race`), TS SDK (type-check, test), Python SDK (`pytest`). Each SDK suite loads every file in `sdks/contract/fixtures` | 1 for `sdks/**`. A Core or gateway wire change with **no** fixture change never triggers it: **4** for wire drift (see `wiring-and-second-item.md`) |
| `allsource-mcp Quality Gates` | `allsource-mcp` | `tooling/allsource-mcp`: fmt, clippy, `cargo test --locked` | 1 |
| `Go Quality Gates` | `go` | `apps/control-plane`: `go mod verify`, `gofmt`, `golangci-lint` v2.11.4, `go test -race ./...`, build | 1 |
| `Elixir Query Service Quality Gates` | `elixir` | `apps/query-service`: format, `deps.unlock --check-unused`, compile warnings-as-errors, `credo --strict`, dialyzer, `mix test` | 1. `List unused Dialyzer filters` is `continue-on-error: true` |
| `Elixir MCP Server Quality Gates` | `mcp` | `apps/mcp-server-elixir`: `apps/mcp-server-elixir/tooling/customer-connection` fmt/clippy/test, then the same Elixir set, plus `live_core_search_test.exs` against `ghcr.io/all-source-os/allsource-core-community:sha-7deb361` | 1. The live test runs against a **pinned** Core image, not the Core in the diff |
| `Quality Gate` | always | summary over `needs` | aggregates; excludes `tenant-isolation` |

## Rows — other PR-triggered workflows

| Check | Trigger | What runs | Notes |
|---|---|---|---|
| `egress-hard-stop.yml` job `proof` | PR + push `main`, paths `apps/core/**`, `apps/control-plane/**`, `apps/query-service/**`, `Cargo.lock` | builds Core `--features multi-tenant`; `cargo test --test egress_budget --test egress_response_limit`; Go `-tags egressintegration -run '^Test(EgressRealCore|ManagedKeyActualCoreRevokeAcrossRestart)'`; `mix deps.get --check-locked` | 1 for egress budgets |
| `perf-bench.yml` `Criterion wall-clock (throughput tracker, non-blocking)` | PR to `main`, paths include `apps/core/**`, `apps/prime-mcp/**`, `apps/chronis/**`, `Cargo.lock`, `tooling/ci-verification/**` | criterion | advisory by its own name |
| `perf-bench.yml` `iai-callgrind (<package>)` | same | instruction counts, compared by `tooling/ci-verification … compare <file> 3 <base-file>` (3% ceiling) | 1 for instruction-count regressions |
| `bandwidth-alerts.yml` `Bandwidth Alert Gates` | PR + push, paths `tooling/monitor-stack/**`, `deploy/prometheus/bandwidth.rules*.yml` | fmt, test, clippy, `cargo deny check` | 1 |

## Not merge gates (tag, schedule, dispatch, `workflow_run`, push-only)

`release.yml` (tags) · `docker-build.yml` (push `main`, tags; job `Verify Core release hard stop and
reads`) · `container-ci.yml` (`workflow_run` after CI) · `security.yml` (push `main`, schedule,
`workflow_run`; `cargo deny check licenses || true` and `go-licenses … || true` are advisory) ·
`disk-alarm.yml`, `seo-evidence.yml` (schedule) · `promote-fly-image.yml` (dispatch) ·
`chronis-pages.yml`, `prime-docs.yml` (push docs paths). These run after a change lands; they never
gate a PR, and an empty `gh run list` for one does not mean it is dead.

## Nowhere — units and suites no workflow runs

Established by `git grep -nE '<unit path>' -- .github/workflows` returning only the rows noted.

| Unit | What exists | What CI does |
|---|---|---|
| `apps/auth` (deployed: `allsource-auth` on Fly) | Rust service, own workspace | **nothing**; no filter key, no job |
| `apps/admin` (deployed: `admin.all-source.xyz`) | Next.js app | **nothing**; not under the `web` key |
| `apps/web` (deployed: `all-source.xyz`) | Next.js app | one test file (row above) |
| `apps/chronis` | Rust CLI, own workspace | only `perf-bench.yml` iai benches; its tests, clippy and fmt run in no workflow |
| `apps/marketing-assets`, `packages/ui` beyond the brand test | — | nothing |
| `crates/hound-extract` | prime-mcp path dep | compiled only when `prime` fires for another path |
| `crates/allsource-prime-models` | optional Core dep (`prime-bundled-model`) | nothing |
| `tooling/*` except `tenant-isolation-check`, `crate-version-check`, `allsource-mcp`, `recall-bench` (build only), `ci-verification`, `monitor-stack`, `fly-disk-alarm` and `seo-evidence` (scheduled runs) | Rust tools | nothing |
| `integrations/langgraph-checkpoint-allsource` | Python package | nothing |
| `apps/core/tests/heap_profile.rs` | `#![cfg(all(feature = "heap-profile", target_os = "linux"))]` | `heap-profile` is in no workflow (`git grep -n heap-profile -- .github` → 0); compiles to zero tests |
| biome lint (`biome.json`, root `package.json` `"lint": "biome check ."`) | config | runs only when a person types `bun run lint`; no workflow invokes `biome` |

## P5 gate tiers, for this repo

1. **Blocking hook** — none exists (no committed hook; see SKILL.md standing facts).
2. **CI-only** — every row above; advisory, because nothing is required.
3. **Runs when a person types it** — biome lint, `cargo test` in any unit in § Nowhere.
4. **Nowhere** — the § Nowhere rows.

A new gate wired only into tiers 3 or 4 is a finding. A new gate wired into `ci.yml` is tier 2;
say so, since nothing above tier 2 exists here.
