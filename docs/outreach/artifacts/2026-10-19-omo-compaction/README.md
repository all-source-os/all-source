# #7183 real compaction evidence

See [evaluation and unsent draft](../../2026-10-19-omo-compaction-evaluation.md) for findings and limits. Native source pin: 93d44a0760448f7f07d2795c6239d51db5ab6576. OpenCode 1.18.22, Bun 1.4.3.

- results-summary.json: source hashes, native event times, stored recovery hashes and byte counts.
- *-round-*.json: native HTTP message snapshots, including full synthetic recovery bodies.
- *-hooks.json: actual skill API receipts and capture/event processing times.
- *-compaction-events.txt: native SSE connection/compaction frames; unrelated event content omitted.
- *-server.log / *-server.err: isolated runtime logs.
- qa-run.ts: native TypeScript integration harness. TypeScript exercises upstream APIs directly; a Rust rewrite would not execute these modules.

## Reproduce

Extract the pinned GitHub archive into QA_ROOT/source, install its locked dependencies with lifecycle scripts disabled, and put the OpenCode 1.18.22 binary at QA_ROOT/runtime/opencode (or set QA_OPENCODE_BIN). Copy qa-run.ts into that source root. Create QA_ROOT/evidence. Supply OPENROUTER_API_KEY through the environment; do not commit or print it. The local-profile fallback in the harness was used on the test machine.

Run serially, each under a 300-second outer deadline:

```text
QA_RUN=bridge QA_ROUNDS=3 QA_LEGACY_FIXTURE=1 bun qa-run.ts
QA_RUN=single-1m QA_SIZES='[1048576]' bun qa-run.ts
QA_RUN=multi-512k QA_SIZES='[65536,65536,65536,65536,65536,65536,65536,65536]' bun qa-run.ts
```

Use fresh QA_RUN names or remove only your own disposable run directories before repeating. The fixed loopback port must be free. Harness reads real messages and SSE, invokes unchanged capture/recovery modules externally, and reaps its server. It does not load the full OMO plugin or inject its compaction context into the model summary.

The recorded bridge fixture body is 1,018 bytes; QA_LEGACY_FIXTURE reproduces its six-byte padding discrepancy. Large fixtures use exact requested byte lengths. The retained harness adds a bounded termination fallback after the measured large-payload run needed a second shutdown signal. This affects cleanup, not capture/recovery or reported message bytes.
