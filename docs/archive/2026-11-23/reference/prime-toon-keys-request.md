# Preserve Prime property keys in TOON responses

## Complete job

An MCP client writes a Prime node through JSON-RPC, closes the process, reopens the same store, requests full node data with TOON output and decodes it without losing or renaming any property. JSON output and persisted values remain unchanged. Ordinary identifier keys retain compact unquoted rendering.

## Observed defect

Installed AllSource Prime 0.22.0 was exercised through real stdio MCP, in isolated local stores with sync disabled by actual stats readback. A node whose properties contain `#policy: "must survive"` remains intact in JSON after process reopen. Its TOON response emits an unquoted `#policy` field. Official `@toon-format/toon 4.4.0` decoder discards that field as a comment. The comparison reports decoded keys `domain,name` while retained JSON also contains `#policy`.

A separate response also emits carriage-return and backslash keys unquoted. The reference decoder accepts those as input; no data-loss claim is made for them. They still violate the encoder's key-quoting contract. Current main `3f6d95ca4951107033b6a4a85a93096456eefc6b` has the same `fmt_key` condition in `apps/prime-mcp/src/toon.rs`; current package is 0.26.1. Installed 0.22.0 observations do not prove a rebuilt 0.26.1 baseline. Rebuild and exercise current baseline before candidate claims.

## Repair boundary

Repair generic TOON key rendering, not node ingestion or properties. [Specification §7.3](https://github.com/toon-format/spec/blob/main/SPEC.md#73-key-encoding) requires unquoted keys matching `^[A-Za-z_][A-Za-z0-9_.]*$` and quoted/escaped keys otherwise, in object fields and array headers. [Escaping §7.1](https://github.com/toon-format/spec/blob/main/SPEC.md#71-escaping) includes backslash, quote, LF, CR, tab and other control characters. Confirm JSON short escapes for backspace/form-feed against TOON before reusing JSON serialization unchanged.

Keep current output formats, API schemas, node IDs and stored property names. No dependency addition, store migration, model/provider change, hosted credentials or deployment is required. This is key integrity qualification, not a claim of complete TOON encoder conformance.

## Acceptance and authority

Freeze a key corpus before implementation: ordinary identifiers, dotted keys, empty/numeric/hyphenated/Unicode keys, comment-prefix keys, structural punctuation, literal escape sequences and controls. Cover nested object fields, scalar-array headers and tabular field headers. Reopen baseline-created data through candidate JSON and TOON transports; compare decoded values with the actual JSON result using the pinned independent reference decoder.

Use real Claude native planning/review/runtime QA and Codex implementation, bounded receipts, one machine compiler and commands below 300 seconds. Existing Prime verification policy covers the application; inspect coverage and supplemental runtime acceptance before adoption. Preserve raw streams outside epoch worktrees before ship, then read back remote branch/PR head, Chronis completion and project Prime outcome.

User authorized `fix/prime-toon-key-quoting` and one AllSource PR for the complete native workflow. Supplied machine instructions override checked-in direct-main defaults: no trunk push and one commit/PR. The baseline is `e947beaff71cbf2ce1257a6b08f0f3b4b6a6ad7a`, whose encoder and Prime policy match the inspected planning source.
