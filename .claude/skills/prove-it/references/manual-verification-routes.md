# Manual verification routes — Phase 4

What a person still owes, by surface. `run` commands were checked to exist at census; re-check
each with `command -v` or `test -e` before printing it.

## Which store — name it in every `read` field

Core is the database, and more than one Core can answer a query. The wrong one returns an empty
result that reads exactly like "no such data".

| Core | Where | Use for |
|---|---|---|
| Production | Fly app `allsource-core` (`fly status --app allsource-core`) | what customers see |
| Local Docker stack | `allsource-core-leader` on port 3280, followers 3281/3282, QS 3283 (repo `CLAUDE.md` § Docker Stack) | reproducing a change before deploy |
| Embedded | an `EmbeddedCore` data dir (chronis, prime-mcp, allsource-mcp) | the embedded editions |

Read a store through its own API (`GET /api/v1/events/query`, `GET /api/v1/entities/{id}/state`,
`GET /health`), never by opening its WAL or Parquet files with another engine. The
`allsource-data-access` skill covers the query shapes.

## Surfaces with a runbook

| Surface | Runbook |
|---|---|
| Core deploy | `docs/runbooks/CORE_DEPLOY.md` |
| Consumer access gate | `docs/runbooks/CONSUMER_ACCESS_GATE.md` |
| Latest-state views | `docs/runbooks/LATEST_STATE_VIEWS.md` |
| Hosted Prime sync | `docs/runbooks/HOSTED_PRIME_SYNC.md` |
| Frontends on Fly | `docs/runbooks/FLY_FRONTENDS.md` |
| Image promotion | `docs/runbooks/FLY_IMAGE_PROMOTION.md` |
| Client key rotation | `docs/runbooks/CLIENT_KEY_ROTATION.md` |
| Memory and compaction | `docs/runbooks/CORE_MEMORY_AND_COMPACTION.md` |
| Read cost | `docs/runbooks/READ_COST.md` |
| System checkpoints | `docs/runbooks/SYSTEM_CHECKPOINTS.md` |
| Admin health | `docs/runbooks/ADMIN_HEALTH.md`, `task admin-health` |

`git ls-files 'docs/runbooks/*'` lists the rest. Route a hand-check to the runbook whose name
matches the claim's surface; when none matches, use the rows below.

## Surfaces with no runbook `[author, forge-prove-it interview]`

### `apps/web` dashboard

```
why    CI runs one test file on apps/web (brand-background.test.ts); no type-check, lint or build
run    bun --cwd apps/web run build
then   no runbook exists — describe the run you did
read   the page's network calls against the Core you named, and the browser console
pass   the changed page renders with live data, and the console shows no failed request
ask    "Which dashboard page did you open against which Core, and what did it show for the changed data?"
```

### `apps/admin`

```
why    no CI job and no filter key cover apps/admin
run    bun --cwd apps/admin run build
then   no runbook exists — describe the run you did
read   admin.all-source.xyz or a local build, and the control-plane logs for the admin request
pass   the changed admin action completes and its effect is readable from Core
ask    "Which admin action did you run, as which operator, and where did you read the result?"
```

### `apps/auth`

```
why    no CI job and no filter key cover apps/auth
run    cargo test --locked --manifest-path apps/auth/Cargo.toml
then   no runbook exists — describe the run you did
read   the auth service's response and the Core auth stream for the account you used
pass   sign-in / token issue for a real test account succeeds, and a revoked or wrong credential fails
ask    "Which account did you sign in with, against which deploy, and what did a wrong credential return?"
caveat one build on the machine at a time; apps/auth has its own Cargo.lock and target dir
```

## Units no workflow tests — the routine hand-check

For a change under a unit in coverage-ledger § Nowhere (except the three surfaces above), the
routine check is its own test suite, run by hand:

```
cargo test --locked --manifest-path <unit>/Cargo.toml
```

`apps/chronis`, `crates/hound-extract`, `crates/allsource-prime-models` and the `tooling/*` crates
each have a `Cargo.toml` and a lock file. This is **routine**: it never gates the verdict.

## Integration suites CI never runs

```
why    heap-profile is enabled by no workflow
run    cargo test --locked --manifest-path apps/core/Cargo.toml --features heap-profile --test heap_profile
then   Linux only (#![cfg(all(feature = "heap-profile", target_os = "linux"))])
pass   the run reports "N passed" with N greater than zero
```
