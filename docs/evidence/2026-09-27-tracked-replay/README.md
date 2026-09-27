# Tracked tenant replay — local verification

Parent: `145d72b862e7dcc05828b2d59b0440c43061cbdd`.
Design: [durable identity and explicit limits](../../plans/2026-09-27-tracked-replay-design.md).

## Verified behavior

Seven integration cases use the actual frozen Core binary through HTTP and private
temporary WAL directories. The existing Query Service projection engine performs
real folds and generation publication. Only source event retrieval is synthetic,
allowing controlled pauses and failures; Core persistence is never mocked.

1. Eight simultaneous copies reserve one identity and dispatch exactly one fold.
   Current state remains visible until publication. Repeated completed requests
   return the same result. Core hard restart and Query Service engine restart
   preserve that result. Reusing an operation for another projection conflicts;
   another tenant cannot read it. A contradictory terminal write fails.
2. A proxy commits a real Core reservation but removes its acknowledgement.
   Neither the first request nor subsequent retries dispatch. After Core restart,
   the same replay identity remains `unknown`.
3. A proxy drops the completion acknowledgement after Core commits. Read-back
   recovers the exact terminal record; retry causes no second fold.
4. Query Service engine restart during a fold retains unknown identity. The old
   worker cannot publish to the replacement process and retries do not redispatch.
5. Cancellation preserves the previous generation. Disabling removes its state.
   Both retain a durable cancellation record; the old worker cannot publish later.
6. Core becomes unavailable after dispatch but before publication. The engine
   publishes once; result reads fail closed while storage is unavailable. Core
   restart lets the existing terminal result be finalized without another fold.
7. Source read failure preserves previous state. The durable/public receipt does
   not contain the raw source exception string.

Focused command: `ALLSOURCE_CORE_BINARY=<frozen binary> mix test
test/query_service_ex/integration/tracked_replays_test.exs
test/query_service_ex/projections/tenant_projections_test.exs
test/query_service_ex/infrastructure/adapters/customer_review_store_test.exs
test/query_service_ex_web/controllers/replay_controller_test.exs --include integration`.
Result: **30 tests, zero failures**, including all seven actual-Core cases.

Full `mix test`: **6 doctests, 1,137 tests, zero failures, 2 skipped, 162 excluded**.
Integration exclusions are not completion evidence. Warnings-as-errors test
compilation, formatting, strict Credo (322 files), and tenant-isolation gate passed.
Dialyzer passed with the existing seven filtered findings and one unnecessary
skip unchanged; no new finding was suppressed.

Core executable SHA-256:
`c4623ef235a683758d3abea8629bc0c5b4a33a9343204b2c944d77c1dcb84d64`.
The source manifest accompanies this report. No UI changes require new screenshots.
The prior product workspace browser evidence remains a separate increment.

## Limits

This is internal operational idempotency, not human approval. No new route, MCP
tool, customer flag, native host installation or deployment is added. The normal
replay API and ingestion are unchanged. No exact tenant-source snapshot, live
operator-role validation, one-use human receipt, source expiry or product
accept/edit/reject gate is claimed.

If publication happened but both the runtime result and final acknowledgement
were lost, the journal remains `unknown`. It never invents success or automatically
replays. Further reconciliation must establish evidence for the same identity.
Completed records describe historical operations, not continued availability of
an ephemeral projection generation. These tests prove process-crash recovery,
not power-loss durability, a production journey or a qualified customer outcome.

The foundation and human-gate beads remain open. Required follow-up is recorded in
the design and existing tracker rather than creating a duplicate delivery queue.
