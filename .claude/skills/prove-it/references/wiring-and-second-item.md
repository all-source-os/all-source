# Wiring and the second item — L5, L6

## L5 · The cross-service chain

Traced from one real identifier, not from prose:
`git grep -l 'read-models' -- apps sdks ':!*.md'` returns the hops below. A Core wire change has
**seven** places to land, in **five** languages, and CI runs them in **different jobs on different
filter keys** (coverage ledger).

| # | Hop | Files (re-grep; these are the `read-models` instances) | Checked by |
|---|---|---|---|
| 1 | Core route + handler | `apps/core/src/infrastructure/web/api_v1.rs` (route table), the handler module | `Rust Quality Gates` (`rust`) |
| 2 | Core auth scope | `apps/core/src/infrastructure/security/middleware.rs` | `Rust Quality Gates` |
| 3 | Control-plane gateway | `apps/control-plane/main.go` (route), the proxy file (`read_model.go`), `egress_guard.go`, `managed_keys.go` (key scope) | `Go Quality Gates` (`go`) + `proof` (`egress-hard-stop.yml`) |
| 4 | Gateway API doc | `apps/control-plane/docs/openapi.yaml` | **nothing** |
| 5 | Query Service client | `apps/query-service/lib/query_service_ex/infrastructure/adapters/rust_core_client.ex` | `Elixir Query Service Quality Gates` (`elixir`) |
| 6 | Contract fixtures | `sdks/contract/fixtures/<name>.json` | `SDK Quality Gates` (`sdks`) |
| 7 | SDKs | `sdks/rust/src/…`, `sdks/go/…` (`latest_state.go`), `sdks/typescript/src/client.ts`, `sdks/python-client/src/allsource_client/…` | `SDK Quality Gates` |

**The unchecked hop is Core ↔ fixtures.** The SDK suites load the fixtures, but nothing loads them
against Core: `git grep -n 'contract/fixtures' -- apps` returns nothing. A Core response that
changes shape while the fixtures stay put leaves every SDK suite green against a contract Core no
longer serves. A Core-only diff never triggers `sdks` at all.

Check, on any diff touching hops 1–3:

1. For each changed route, response type or JSON field name, grep the **wire string** (path
   segment, field name as serialized) across hops 3–7.
2. Each hop that should change and did not → finding. `BLOCKS-CLAIM` when the claim names that
   consumer; `SILENT-RISK` otherwise.
3. **Both directions.** A new SDK method or fixture with no Core route behind it is the same defect
   reversed: grep the path in `api_v1.rs`.
4. A fixture changed with no Core change → ask which Core head produced the new shape.

Env var chain (QS): `CORE_URL`, `CORE_WS_URL` → `config :query_service_ex, :core_url` →
`RustCoreClient`. Control plane: `CORE_SERVICE_URL`, `CORE_API_KEY` → `NewCoreClient`
(`core_client.go`). A new env var must be set in the app's `fly.toml` or as a Fly secret, and named
in the runbook that deploys it; a var read and never set is P8.

## L6 · The four questions — for every loop, batch, queue, retry, lease, slot or state transition

1. **What happens to the second element?** Not the first. The second, and the last. Is anything
   about element *n* derived from a bound element 1 happened to satisfy — a length cap, an index,
   a preallocated buffer, an id built from a growing prefix?
2. **What happens when one element fails?** Does the failure abort the batch or skip the element?
   Is the error attributed to the element that produced it, or to whatever the loop variable
   happens to hold? Does a partial failure leave the batch half-committed?
3. **What happens on the exit paths — every one of them?** Enumerate the states that release a
   resource and the states that hold it, and confirm the diff's new state is in exactly one list.
   A new terminal state, a new park, a new review gate and a new abort are each a door.
4. **What happens when it runs twice, or concurrently?** Is the id deterministic, and is that
   intended idempotency or an accidental swallow? Does a second concurrent run see the first's
   partial state? Is "already exists" handled as success or retried?

L6 triggers on any state decision, not only loops: a function returning one of several states, a
status enum, an early-return guard, an error arm. *Which states reach that branch and never leave
it?*

### This repo's incidents, under the question they answer

**Q1 · the boundary element.**

> Sync pulls passed the peer's last-seen timestamp straight into the query's inclusive `since`, so
> every pull re-sent every event sharing that millisecond, at ingest speed, forever (`da458672`).
> The suite that would have caught it had never executed.

Check: a cursor, offset, `since`, `after` or watermark the diff touches — is the bound inclusive or
exclusive, and does the element **at** the bound get delivered once?

**Q3 · the exit paths.**

> Prime called `hydrate_all_from_storage` after `EmbeddedCore::open` had already hydrated, so every
> durable Prime boot read the whole Parquet archive twice (`f1c9fd5b`). Hydration had moved into
> `open` for all embedded consumers without removing Prime's copy.

Check: when a diff moves a step (hydrate, replay, backfill, compaction, flush) to a new owner,
grep the step's function name for callers that still run it themselves.

**Q4 · run twice.**

> An amended, force-pushed commit orphaned its `before` SHA; `paths-filter` reported no paths
> changed, every language gate skipped, and a Credo failure reached `main` (`ci.yml` comment above
> `Verify diff base is reachable`).

Check: any job or step whose decision depends on a diff base or a previous run's state — what does
it do when that state is absent?

### Second half — a fix that names no invariant

On a `fixes-defect` claim:

**(a) Is the invariant asserted anywhere it can be re-violated?** Ask the author to state it in one
sentence, then ask where it is asserted. Files fixed repeatedly in the last six months on `main`
(`git log origin/main --since=6.months -E -i --grep='^fix' --format= --name-only -- apps crates sdks | sort | uniq -c | sort -rn`)
include `apps/core/src/store.rs`, `apps/core/src/infrastructure/persistence/wal.rs`,
`apps/core/src/infrastructure/persistence/erasure*.rs`,
`apps/core/src/infrastructure/persistence/archive_repack.rs` and `apps/core/src/embedded/core.rs`.
A fix to one of these without a named, asserted invariant is `sand` at least.

**(b) Do the sibling call sites have the same bug?** One grep: the other fields of the same struct,
the other arms of the same match, the other callers of the same helper, the other gateway (a fix
in `apps/control-plane` usually has a twin in `apps/query-service`, and a fix in one SDK has three
twins). Fixed one of several → finding; name the ones left.
