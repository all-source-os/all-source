# Offline evaluation — 2026-09-23

Command: `cargo run --manifest-path tooling/jev-eval/Cargo.toml --offline`.

Eight hand-authored synthetic holdout cases (separate from transport test
fixtures). Frozen rule: invoice/payment keyword means Billing, otherwise Access.

| Measure | Rules | Jev |
| --- | --- | --- |
| Correct | 5 / 8 | Not run |
| Accuracy | 62.5% | Not measured |
| Access false negatives | 2 | Not measured |
| Provider p95 | Not applicable | Not measured |
| Provider cost | $0 | $0 — no calls |

Rule errors: s3 and s7 are access problems with incidental billing vocabulary;
s6 requests a refund without either billing keyword. These illustrate the
evaluation shape, not demand evidence or a representative customer error rate.
No Jev output was simulated as if it were real.

Adapter contract tests exercise fake transports, including never-resolving calls
with paused Tokio time. They do not measure HTTP latency or provider quality.
The optional HTTP implementation compiles; no remote integration test ran.

Live runs retain token usage but cannot report an accurate dollar total until
current pricing and potentially charged failed attempts are accounted for.
No threshold tuning was performed. Confidence 0.8 is an experimental policy.

Remaining epic gate: compare optional reranking to actual Prime/MMR on a fixed
labelled retrieval set, retaining local fallback. Keep epic open until that work
is verified. Production tenant authorization and content redaction are outside
this synthetic prototype.
