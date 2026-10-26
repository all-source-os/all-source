# Latest-state views and the consumer-access gate, run end to end

PR #366 rebased onto main `3f6d95ca`, which includes the key-authority shadow
mode from `5835e7b7`. Code under test is the tree of `f912b50a7a`; the commit
that carries this report changes only `docs/`. Run on 2026-10-27 against
locally built Core, Query Service and control plane, wired the way production
wires them: every customer call goes through the control-plane gateway, Core
holds the rows, the Query Service folds them. The control plane ran with
`ALLSOURCE_KEY_AUTHORITY_MODE=shadow`, the production default.

## 1. A customer can define a view and read its latest rows through the gateway, from the Python, TypeScript and Go SDKs

A tenant that signed up through the gateway, holding a dashboard key with
`projections:write`, defines `ops_status`, ingests events, and reads the folded
rows with `latest`, `latest_many` and `sync`. Before this PR's SDK change the
TypeScript and Go SDKs got `401` at the gateway on every call; with it they
read the same rows ([`sdk-gateway-auth.txt`](sdk-gateway-auth.txt)).

## 2. Before and after

| | `e1416e58eb` (old base) | This PR on `3f6d95ca` |
|---|---|---|
| `PUT /api/v1/views/ops_status` | no such route in Core, gateway or SDKs (read from source, not run) | `200`, generation 1 |
| Latest state of one key | fold the entity's events client-side, or `/entities/{id}/state` with `history[]` | `latest` returns the row; with `If-None-Match`, `304` and 0 body bytes |
| Following changes | poll `/events/query` | `sync` long-poll delivered a new row 0.03 s after its event was ingested |
| Gateway readiness | `/livez` only | `/readyz/consumers` and `/api/health/consumers`: `503` until every canary kind reads |
| TypeScript and Go SDKs through the gateway | `401 "no authorization header"` (run with the SDKs at `27e9296c`) | `latest`, `latestMany` and `sync` succeed |

## 3. What the run showed

Full step log with timestamps: [`steps.jsonl`](steps.jsonl). Numbers below
come from that log.

**Signup to key, through real endpoints.** `POST /api/v1/auth/register` on the
gateway created tenant `ws-a6a0…f8c10b`. `POST /api/api-keys` on the Query
Service minted a key with `events:read events:write projections:read
projections:write` for that same tenant.

**Fold.** `define_view` returned generation 1, `ready: false`. Five events
were ingested: four `ops.probe.observed` and one `billing.invoice.paid`
outside the view's prefix. The view became `ready` at revision 1 after 12.1 s,
inside the `ViewCatalog` 30 s discovery interval. Row `probe-a:2026-10-27`:

```json
{"day": "2026-10-27", "failed": 1, "last_seen_at": "2026-10-27T17:06:17.444280Z", "ok": 2, "probe": "probe-a", "status": "degraded"}
```

Two `ok: true`, one `ok: false`, last status wins, billing event ignored.

**Conditional reads.** `latest` with the returned ETag
`"sha256:b5394f2b5c7debb5d941fad2421ff65b"` came back `NotModified`; the raw
gateway response was `304`, 0 body bytes, same ETag. `entity_state("probe-a")`
returned `version: 3` and no `history`; its ETag round-tripped to
`NotModified` too.

**Sync.** The snapshot yielded 2 upserts and `SyncCheckpoint(revision=1,
generation=1)`. A follower resumed from it with `wait=15`; one `probe-c`
event ingested 2 s later reached the follower 0.03 s after the ingest call
returned.

**Refusals.**

| Attempt | Result |
|---|---|
| Tenant `PUT /api/v1/read-models/view_ops_status` | `403 {"error":"reserved_read_model"}` |
| Read-only key (`events:read projections:read`) `PUT /api/v1/views/ro_probe` | `200`, view created. The gateway logged `API key authority would refuse: mode=shadow … reason=scope` (see section 5) |
| Tenant B reads A's view before defining its own | `404 view_not_found` |
| Tenant B defines a view with the same name | folds only `probe-q:2026-10-27`, B's one event, in 24.4 s |
| Tenant B reads `probe-a:2026-10-27` | `ViewNotFound` |
| Tenant B passes `?tenant_id=<A>` | `404 {"error":"not_found","ready":true}`: the gateway stripped it |

**Core restart.** Core was stopped and started on the same data directory
with `ALLSOURCE_FEED_DURABLE_LOG=all`. Feed epoch before and after:
`3438269401613255`, head position 10 both times. The view kept revision 2;
`view_changes(after=1)`, a cursor taken before the restart, returned `probe-c`;
one more `probe-a` event moved `ok` from 2 to 3 within 1.0 s, so no double
count and no rebuild.

**Consumer-access gate.** Raw responses:
[`consumer-gate.txt`](consumer-gate.txt). Both gateways answer `503` with
`"no canary credentials configured"` when the secret is unset. With an
onboarding key, a managed key without expiry and a managed key with expiry,
both answer `200`, 3 of 3. Adding a revoked managed key turns both to
`503 … revoked_managed_key status 401`, while `/livez` on the same process
still answers `200`: the 2026-10-26 failure, liveness green and a credential
kind locked out, now reported by the gate. Shadow mode still refuses revoked
keys, so the gate keeps its meaning under the production default.

**TypeScript and Go SDKs.** Raw output:
[`sdk-gateway-auth.txt`](sdk-gateway-auth.txt). Both SDKs sent only
`X-API-Key`; the gateway reads only `Authorization: Bearer`
(`apps/control-plane/auth.go`, `ExtractToken`). With the SDKs at `27e9296c`,
the same key the Python SDK used got `401 "no authorization header"` from
TypeScript `latest`, `latestMany`, `sync` and Go `Latest`, while a raw Bearer
request with it got `200`. This PR makes both SDKs send
`Authorization: Bearer <key>` and keep `X-API-Key` for callers that reach the
Query Service directly. On the rebased tree TypeScript returned the `probe-a`
row, three keys from `latestMany` and a `sync` snapshot of 3 upserts at
revision 3; Go returned the same row, keys and checkpoint. A raw
`X-API-Key`-only request is still `401` there.

## 4. Prove it yourself

From the repository root on this PR's head. Run `core`, `qs` and `cp` each in
its own terminal.

1. Build Core with the production feature set (`apps/core/Dockerfile`, runtime stage):
   ```bash
   cargo build -p allsource-core --bin allsource-core --features enterprise,analytics
   ```
   Pass: `target/debug/allsource-core` exists. Copy it out of `target/` and pass `CORE_BIN=<copy>` to step 3 if anything on the machine evicts build directories.
2. Generate local secrets and build the control plane:
   ```bash
   bash docs/evidence/2026-10-27-latest-state-views/harness/stack.sh init
   ```
   Pass: prints `state: <dir>`.
3. Start Core:
   ```bash
   bash docs/evidence/2026-10-27-latest-state-views/harness/stack.sh core
   ```
   Pass: `curl -s 127.0.0.1:4900/health` shows `"status":"healthy"`.
4. Start the Query Service:
   ```bash
   ALLSOURCE_KEY_AUTHORITY_MODE=shadow bash docs/evidence/2026-10-27-latest-state-views/harness/stack.sh qs
   ```
   Pass: `curl -s 127.0.0.1:3902/api/health/live` answers 200.
5. Start the control plane:
   ```bash
   ALLSOURCE_KEY_AUTHORITY_MODE=shadow bash docs/evidence/2026-10-27-latest-state-views/harness/stack.sh cp
   ```
   Pass: `curl -s 127.0.0.1:4901/readyz/consumers` answers 503 `no canary credentials configured`.
6. Sign up two tenants and mint keys:
   ```bash
   uv run --project sdks/python-client python docs/evidence/2026-10-27-latest-state-views/harness/e2e.py setup
   ```
   Pass: every line has `"passed": true`.
7. Define, fold, read, sync:
   ```bash
   ALLSOURCE_KEY_AUTHORITY_MODE=shadow uv run --project sdks/python-client python docs/evidence/2026-10-27-latest-state-views/harness/e2e.py e2e
   ```
   Pass: every `passed` is `true`, and `Query Service folded the view` within 35 s.
8. Tenant isolation:
   ```bash
   uv run --project sdks/python-client python docs/evidence/2026-10-27-latest-state-views/harness/e2e.py isolation
   ```
   Pass: every line `"passed": true`.
9. Restart Core (stop step 3, run it again), then:
   ```bash
   uv run --project sdks/python-client python docs/evidence/2026-10-27-latest-state-views/harness/e2e.py after_restart
   ```
   Pass: `fold resumes after restart without double counting` shows `"ok": 3`.
10. TypeScript SDK against the gateway:
    ```bash
    bun run docs/evidence/2026-10-27-latest-state-views/harness/ts-sdk-probe.ts
    ```
    Pass: `"target":"gateway"` shows `"ok":true`; `latestMany` and `sync` print `"ok":true`.
11. Go SDK against the gateway:
    ```bash
    go run -C docs/evidence/2026-10-27-latest-state-views/harness/go-sdk-probe .
    ```
    Pass: `Latest`, `LatestMany` and `Sync` each print `"ok":true`.
12. Mint canaries:
    ```bash
    uv run --project sdks/python-client python docs/evidence/2026-10-27-latest-state-views/harness/e2e.py canaries
    ```
    Pass: `canaries-good.json` and `canaries-bad.json` appear in the state dir.
    Copy either to `canaries.json` there and restart steps 4 and 5. Pass: `200` with the good set; `503` naming `revoked_managed_key` with the bad set.

The harness is Python because the SDK it drives end to end is the Python one.
State (secrets, data, logs) goes to `$PR366_STATE`, default `$TMPDIR/pr366-proof`.

## 5. Traps

- **In shadow mode a read-only key can define and delete views.** Main's
  `5835e7b7` serves managed-key scope refusals in shadow and only counts them,
  and shadow is the production default. `projections:write` on `PUT|DELETE
  /api/v1/views/*` is therefore logged, not enforced, until the gateways move
  to `enforce`; a `projections:read` key created `ro_probe` with `200`. Core
  does not check `projections:*` scopes itself. On the pre-rebase head, where
  the gateway always enforced, the same request was `403 insufficient API key
  scope`. Reserved read models (`view_*`) stay refused in every mode.
- **An SDK unit test cannot catch a header the gateway ignores.** The
  TypeScript and Go suites mock HTTP and stayed green while both SDKs got `401`
  at the real gateway. The contract now states the header
  (`docs/contracts/latest-state/README.md`) and each SDK's tests assert it, but
  only a run against the gateway, like the two probes, proves it. Published
  TypeScript and Go SDK versions still send `X-API-Key` only.
- **Onboarding keys cannot define views even in enforce.** `IssueManagedAPIKey`
  (`apps/control-plane/managed_keys.go`) grants `projections:read` and not
  `projections:write`. Dashboard keys can carry it. Whether onboarding keys
  should is a product decision.
- **A Core built with default features has no tenant routes.** `POST
  /api/v1/tenants` is behind `multi-tenant`; gateway signup then fails with
  `workspace provisioning unavailable`, which reads like a gateway bug. Build
  with `enterprise,analytics` as production does.
- **The Query Service edition is compile-time.** Compiled as `community`, it
  pins every request to the `community` tenant, so dashboard keys come out for
  the wrong tenant. `ALLSOURCE_EDITION=enterprise` must be set when it compiles.
- **`ready: false` for up to 30 s after `defineView` is normal.** `ViewCatalog`
  discovers new definitions on a 30 s interval; the run saw 12.1 s and 24.4 s.
- **`core-direct` stays `401`.** Customer keys are accepted at the gateway, not
  by Core directly; the probe prints it to show which surface answered.

## 6. What this proves that the tests cannot

The PR's suites mount Core's routers in-process with an injected principal
(`apps/core/tests/view_api_http.rs`), fake Core in the Query Service tests, and
mock HTTP in every SDK; none crosses a process boundary. This run shows four
things they cannot:

- The gateway's tenant forcing, scope mapping and 304 pass-through compose with Core's view routes and the Query Service's act-as calls.
- The `system`-tenant service principal works when created the way production creates it, by bootstrap env on first boot.
- The `changes` cursor and the feed epoch survive a real process restart.
- The consumer gate fails on a credential that a real Core refuses, under the shadow mode main now ships.

It also found the SDK header mismatch, which every mocked suite hid, and the
unenforced view-write scope under shadow, which only appears when this PR and
main's key-authority change run together.

## 7. What I have NOT checked

- **Production.** Nothing was deployed. The gate's effect on a Fly bluegreen
  deploy (`[checks]` blocking promotion) is untested; only the endpoint
  behaviour is shown.
- **A key issued before 2026-10-26** through the old `SignAPIKey` path. The
  canaries were minted by this build, so they show the gate's mechanics, not
  backward compatibility with old keys.
- **The Query Service in shadow.** Its key-authority mode is read only in the
  `prod` config block, so the dev build ran in its default, `enforce`. The
  control plane ran in shadow.
- **Release-profile binaries.** Core ran as a debug build; timings are
  indicative only.
- **Only reads were run from TypeScript and Go.** Their `defineView`, ingest
  and `sync` long-poll were not; the Python SDK ran those. The Rust SDK and
  both MCP servers were not run.
- **Gateway `changes` lane limits** (64 global, 8 per tenant, `429 pool_full`)
  were not exercised under load.
- **`ALLSOURCE_FEED_DURABLE_LOG` off.** The restart ran with the log on; the
  documented `409 version_conflict` rebuild path without it was not run.
- **Core's legacy `/api/v1/auth/register` users did not survive a Core
  restart** in an earlier local run of this harness: login was `200` before
  and `401` after. Production sets `AUTH_SERVICE_URL` on the control plane,
  so production sign-in does not use that path, and this PR does not change
  it. Not investigated further.
- **The old base was not run.** The "before" column for views is read from
  source; the SDK "before" was run.
