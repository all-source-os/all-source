# Reject zero Prime sync intervals before startup

Status: CURRENT

## User outcome

An operator who supplies a zero sync interval receives a configuration error before Prime opens its store. Valid configurations continue serving MCP tools and transferring events through the existing sync loop.

## Observed defect and source owner

The installed `allsource-prime 0.22.0` accepted `--sync-interval-ms 0` against a disposable store and a loopback-only destination. Its sync worker panicked with `` `period` must be non-zero. ``, while the process answered `prime_stats` with `sync.enabled: true` and exited 0. This is supporting runtime evidence from an older installed release. Current-main runtime behavior must be exercised with a newly built base executable before claiming a regression comparison.

Fetched AllSource main `897623e8` still declares `sync_interval_ms: u64` without a range in `apps/prime-mcp/src/main.rs`. Server startup passes that value to both `sync::run_sync_loop` and `email_ingester::run_email_ingest_loop`; each constructs a Tokio interval. The CLI parser owns the invalid configuration prerequisite, before store initialization or detached task creation.

Real authenticated Claude invoked native `/allstack-plan` in this checkout. Session `65266a08-99ef-464c-98e0-5f7440350d1d` expanded the method and performed ten read-only tool calls without permission denials. Raw host evidence remains in the founder-mode workspace's ignored `.allstack/proof/allsource-native` directory. The implementation contract below uses process-level tests to exercise the public interface rather than private parser access or process-global environment mutation.

## Repair

Add `clap::value_parser!(u64).range(1..)` to the existing argument declaration. Retain the field type, default 1000 milliseconds, environment variable, and positive-value behavior. Describe the minimum in CLI help and the existing Prime README. Do not clamp invalid values in either loop.

Disabling sync remains the existing omission of the sync URL/key pair. No zero-as-disable contract appears in the current argument declaration, README, server metadata, or extension-building source. Uniform argument validation will also reject zero in one-shot modes; those modes have no documented zero-specific behavior.

## Files and boundaries

- `apps/prime-mcp/src/main.rs`: declarative parser constraint and help text.
- `apps/prime-mcp/tests/sync_interval.rs`: external process tests of the actual binary, with isolated environment, store directory, stdin, stdout, stderr, and bounded child ownership.
- `apps/prime-mcp/tests/sync_delivery.rs`: actual MCP node creation/readback, loopback event delivery, email polling, and persisted acknowledgement cursor under one deadline.
- `apps/prime-mcp/README.md`: supported configuration and observable invalid-input behavior.
- `.github/workflows/ci.yml`: execute external process tests in the existing Prime quality job, with a bounded command.
- This indexed plan and the eventual indexed verification report.
- Installed native skill packages, delivery/catalog definitions, and a scoped `workflows/verification.toml` retain the project workflow and exact-commit gates.

No new dependency is needed. Keep sync mechanics, email ingest, persisted cursors, hosted configuration, and protocol schemas unchanged. Reuse existing test libraries for an isolated loopback endpoint if needed; do not contact a hosted tenant.

## Executable acceptance

Freeze acceptance before implementation and execute the same cases against fresh base and candidate builds. Every test child has one deadline, and timeout cleanup kills then reaps it. Environment differences belong on each child command, never in the test runner's global environment.

| Case | Required observation |
| --- | --- |
| Explicit `--sync-interval-ms 0` | Exit 2; stderr names invalid argument; no store created or remote contacted. |
| `PRIME_SYNC_INTERVAL_MS=0` without explicit value | Same rejection before startup. |
| Malformed, negative, and overflowing values | Nonzero parse error, empty protocol output, no store creation. |
| Positive explicit value overriding invalid environment | CLI precedence retained; genuine MCP initialize and tool responses succeed. |
| Omitted interval | Default remains 1000; actual local-only MCP flow succeeds. |
| Positive environment value | Genuine MCP operation succeeds. |
| Help and version | Exit 0 without opening a store, including with invalid interval environment if clap preserves its normal short circuit. Verify the observed behavior rather than assume it. |
| Valid synced MCP session | Create a node through the actual MCP tool, read it back through Prime, observe its event at an isolated loopback endpoint, and confirm no interval panic. |

Protocol responses must be parsed and checked, not inferred from process status. A `sync.enabled` flag alone does not prove delivery. The loopback endpoint proves transfer to the test boundary; it does not prove hosted Core or dashboard behavior.

## Native workflow and publication

Claude plans, Codex implements the accepted scope, and Claude independently reviews the diff and performs CLI/MCP QA through native Allstack skills. Updated Allstack runtime receipts retain expected and actual exits, separate streams, executable digests, and deadlines.

Use the owned isolated checkout as durable coordinator because the primary AllSource checkout contains unrelated work. Preserve existing Chronis history and source-linked Prime context. Stage only intended source and generated native-method paths; enroll the native candidate through Allstack adoption, verify the exact single commit, and ship one authorized PR. Archive path-bound receipts before removing this owned checkout.

Machine policy forbids trunk pushes and requires explicit branch permission, taking precedence over checked-in direct-main guidance. The user authorized `fix/prime-sync-interval` and its PR. No release, deployment, global Prime replacement, or merge is included.

## Verification limits

Planning and installed 0.22.0 evidence do not establish current-main or candidate runtime behavior. Fresh base and candidate runs, independent runtime QA, and publication readbacks remain acceptance requirements.
