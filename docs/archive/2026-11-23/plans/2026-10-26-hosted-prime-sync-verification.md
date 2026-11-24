# Hosted Prime sync verification

## Goal and ownership

Prove that a local Prime MCP write reaches the authenticated hosted AllSource tenant and can be read independently by another coding host. Native Allstack skills guide planning, QA, review, and shipping. AllSource owns event history; Chronis owns delivery claims; Prime retains source-linked context.

Use the merged Prime implementation from `54c26ea308f38b2adf6b68e3609d6f23b8c2e8af`. Production code remains unchanged unless the real flow identifies a defect. The existing dirty primary checkout is outside the candidate; use the authorized `test/hosted-prime-sync` branch in the isolated coordinator.

## Runtime boundary

The public origin is `https://api.all-source.xyz`. Local Prime uses `PRIME_SYNC_TO` and `PRIME_API_KEY`; credentials are opaque strings, including the JWT-format keys issued by the hosted dashboard. The gateway derives tenant identity from authentication, injects it into event ingestion and query requests, and forwards trusted identity to hosted Prime. The local writer must never supply another tenant ID.

Use only the authorized temporary key with `events:read` and `events:write`, a 30-day expiry, and revocation after qualification. Pass its value through child environment only. An optional private key-file path lets the test harness load it without exposing the value to either coding host or command arguments. Keep raw receipts and local proof stores private and ignored.

## Executable acceptance

`apps/prime-mcp/tests/hosted_sync.rs` contains two explicitly ignored integration tests. `tests/hosted_sync/remote.rs` owns hosted HTTP requests and receipt types. Existing package dependencies suffice.

1. Rebuild Prime from the intended source with `--no-default-features`. Record the Cargo artifact as freshly built, its resolved path, and SHA-256. Compile the unit/integration test targets with the same feature/profile fingerprint. Acquire the machine compiler slot before each compilation and bound every command below 300 seconds.
2. Run `hosted_prime_sync_write_restart` through authenticated Codex using native `allstack-qa`. The actual MCP binary creates a uniquely named synthetic decision in an isolated local store. Poll the hosted exact-entity event query and persisted local cursor. Assert one matching event with exact properties and a valid remote UUID.
3. Close and reap the local process. Reopen the same store, read the first decision through MCP, wait for the configured sync loop, and assert the first remote event ID remains unchanged. Create a second decision, require a later persisted cursor, and assert both unique remote IDs remain stable after shutdown.
4. Run `hosted_prime_sync_independent_readback` through authenticated Claude using native `allstack-qa`. This process opens no writer store. Read the sanitized receipt, query both exact entities through hosted Core, and fetch both exact nodes through hosted Prime. Assert matching identities, properties, and one remote event per entity.
5. Missing and invalid authentication must return HTTP 401. The writer checks GET and POST; the independent reader checks GET only. Check the hosted Memory UI for the two named proof decisions.
6. Revoke the temporary key and verify subsequent authenticated access fails. Keep synthetic decisions in the audit trail; do not delete unrelated tenant data.
7. Use native `allstack-ship` to adopt the accepted candidate, run the frozen package policy on its one final commit, publish one PR, and read back GitHub, Chronis, and Prime state.

## Failure behavior

Both hosted tests require explicit `--ignored`. Missing configuration fails before a store opens. HTTP requests have a 10-second deadline and reject redirects. Each test has one 120-second overall deadline. The writer owns every child handle and kills then reaps on drop. A remote rejection, missing node, unexpected event, duplicate event, changed properties, unadvanced cursor, timeout, or unsuccessful child exit fails the proof. Retain separate private MCP stdout/stderr and bounded command receipts.

A passing wrapper or `sync.enabled` flag does not prove delivery. Inspect actual hosted rows, graph nodes, process outcomes, and executed test counts. Ignored tests in ordinary CI are not hosted qualification.

## Compatibility and limits

No production API, CLI default, source store, or persisted production format changes. Existing Prime tests and package lint/format gates remain required. The proof runs Codex and Claude sequentially on one machine; another physical computer, a second tenant, hosted-server restart durability, and vector retrieval are outside this acceptance. Global MCP Prime remains local-only and is distinct from the hosted tenant. Preserve existing project Prime context and Chronis history.
