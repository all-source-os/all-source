# Prime TOON key integrity through native delivery

## Complete user job

An MCP client creates a memory node through JSON-RPC, closes the process, reopens the same Prime store and requests full node data in TOON. An independent decoder returns every original property name and value. JSON results and stored properties remain unchanged; ordinary identifier keys keep compact output.

The [application request](../reference/prime-toon-keys-request.md) records the reproduction and boundaries. The user authorized `fix/prime-toon-key-quoting` and one AllSource PR for this complete native delivery.

## Source and observed behavior

Baseline AllSource source is `e947beaff71cbf2ce1257a6b08f0f3b4b6a6ad7a`; Prime package version is 0.26.1. The source plan inspected identical encoder and policy content at `3f6d95ca4951107033b6a4a85a93096456eefc6b`. The installed Prime executable used for initial supporting observations identifies itself as 0.22.0. Those observations do not replace a freshly rebuilt 0.26.1 baseline.

Real isolated stdio MCP calls create a node with `#policy: "must survive"`. JSON readback after process reopen preserves that property. TOON emits its key unquoted. Pinned official `@toon-format/toon 4.4.0` decoding drops the property as a comment: decoded properties contain `domain,name`; retained JSON also contains `#policy`. Native command receipt `9374e3bb-206b-4989-a6e1-f1d80a9a7c86` records the expected negative comparison exit. Stats before writing show an empty local store with synchronization disabled.

A separate response emits carriage-return and backslash keys unquoted. The reference decoder accepts both, so data loss is established for the comment-prefixed key only. Their output also violates the encoder's key contract.

Authenticated Claude session `3660e9d5-d0b5-4236-b9c9-8b482c46e30c` read the installed native planning method, request and current source/policy using 14 observed Read/Grep/Glob calls. Its completed source plan is retained privately. Planning performed no application execution.

## Owner and repair

The presentation encoder owns the defect. `apps/prime-mcp/src/toon.rs::fmt_key` serves object fields, scalar-array headers and tabular field headers. Its punctuation denylist leaves comment prefixes and other non-identifier keys bare. `tools.rs::tool_result` wraps encoded data in the normal JSON-RPC text result; that envelope remains unchanged.

Use the [TOON §7.3 key allowlist](https://github.com/toon-format/spec/blob/main/SPEC.md#73-key-encoding): bare keys match `^[A-Za-z_][A-Za-z0-9_.]*$`; every other key is quoted. Quoted keys follow [§7.1 escaping](https://github.com/toon-format/spec/blob/main/SPEC.md#71-escaping). A single character scan emits backslash, quote, LF, CR and tab escapes; other C0 controls use lowercase `\\uXXXX`. JSON's backspace/form-feed short escapes are outside that TOON escape contract, so unmodified JSON string serialization is insufficient for the new key path.

Implement this at the existing key renderer with standard-library operations. Node ingestion, JSON serialization and retained property names keep their current behavior. This qualifies key integrity within existing supported shapes; it does not claim complete TOON specification conformance.

## Files and compatibility

- `apps/prime-mcp/src/toon.rs`: key selection and TOON-safe quoted-key rendering.
- Sibling encoder test file(s): move existing inline test module intact when touching the encoder, and add corpus tests outside inline modules.
- `apps/prime-mcp/tests/` and a fixture below its existing layout: bounded real-binary process-reopen acceptance, using existing dependencies.
- Indexed plan, request and final verification report under `docs/`.

Existing string-value policy stays outside the key repair. Preserve ordinary identifier/dotted-key output and existing wire-layout tests. Quotes added around other keys preserve their decoded names. A sanitizer at ingestion would alter all consumers and leave the serializer's contract unresolved.

## Frozen acceptance

Before implementation, the pinned reference encoder/decoder generated and round-tripped 132 cases: 33 keys across object fields, nested fields, scalar-array headers and tabular field headers. Corpus SHA-256 is `465d52721c1964c28dbb16b1a95eeb294501d3fb734977a2e4261bfaf42e3809`. It includes identifiers, dotted names, empty/numeric/hyphenated names, comment prefixes, punctuation, Unicode, literal escapes and C0 controls including backspace/form-feed. The same bytes must exercise rebuilt baseline and candidate.

The reference package is dependency-free and pinned by its registry SHA-512 integrity. Bun executes its published JavaScript for independent interoperability qualification in ignored proof storage. Application Cargo dependencies remain unchanged.

## Build and runtime proof

1. Seed an owned build directory from a suitable warm cache, verify copied prerequisites and record the toolchain/features. Wait for one exclusive machine compiler. Bound each command below 300 seconds.
2. Rebuild current baseline source with the frozen tests. Record actual Cargo artifact paths and executable digest; cloned cache timestamps do not prove provenance. Execute the baseline acceptance and record its failures.
3. Have real Codex implement the accepted repair. Rebuild the candidate from its actual checkout, then execute the identical corpus.
4. Seed node data using the current-source baseline JSON MCP path. Close that producer. Reopen the retained store with the candidate, read stats through Prime, request full JSON and TOON results, and compare the independent decoded output with actual JSON key-for-key and value-for-value. Include ordinary compatibility and comment/control-key cases.
5. Have real Claude read installed review/QA methods and current source, then execute candidate acceptance independently. Retain actual calls, expected/observed exits, separate raw streams and executable/source identities. Model summaries alone do not establish execution.

The existing `workflows/verification.toml` targets Prime formatting, executed binary/integration tests, all-target Clippy and whitespace. The frozen corpus belongs in that executed test surface. Independent decoder/MCP process-reopen acceptance is supplemental; compare CI coverage with those requirements before publication.

## Native continuation and publication

Preserve repository Chronis history and relevant source-linked project Prime context. Adopt the completed native candidate with this accepted plan and the Prime policy, using only authority granted by the user. Verify one commit against the frozen policy. Use a conventional message explaining the output-integrity reason.

Before native ship, archive epoch-local receipts and all referenced streams outside disposable worktrees and verify byte lengths/digests. Ship to the single authorized PR, read back remote branch and PR head, and confirm Chronis completion and project Prime plan/review/QA/outcome through their engines. Archive isolated coordinator state before removing owned worktrees and idle build directories.

Supplied machine Git discipline overrides checked-in direct-main defaults: no trunk pushes and no new named branch without explicit permission. Merge, deployment and hosted credentials are outside this requested local application qualification.
