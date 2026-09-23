# Jev evaluation prototype

Standalone Rust application tool; no Core dependency or production wiring.
API contract checked against https://docs.typesafe.ai/api on 2026-09-23.
Supports a single support-routing Choice question, not arbitrary primitives.

## Offline default

```console
cargo test --manifest-path tooling/jev-eval/Cargo.toml --all-features
cargo clippy --manifest-path tooling/jev-eval/Cargo.toml --all-features --all-targets -- -D warnings
cargo run --manifest-path tooling/jev-eval/Cargo.toml
```

Default features exclude HTTP. Even with `live` compiled, the adapter defaults to
disabled and the CLI stays offline without an explicit flag. Offline output
measures a frozen lexical baseline against eight hand-authored synthetic cases;
these cases are separate from contract fixtures but not an independently curated
benchmark. Do not generalize their accuracy to customers.

## Optional live evaluation (not run)

After approval for provider charges and synthetic transfer, set
`TYPESAFE_API_KEY` securely in the process environment, then run:

```console
cargo run --manifest-path tooling/jev-eval/Cargo.toml --features live -- --live-synthetic
```

This sends only the bundled synthetic texts; no CLI file argument or customer
data source exists. Up to eight requests, each with at most one retry after
429/529 and 250ms backoff. Each attempt has a 5s deadline; timeout is not retried
because the provider might already have charged it. No redirects. Response body
is capped at 64 KiB. Error bodies and keys are not printed. A timeout or retry
can still incur cost; token usage only represents successfully returned answers.

Live p95 is whole-call latency including retry time. Cost remains null because
no verified rate is configured; output retains token usage. Offline cost is zero
and live accuracy/latency remain null. Abstention and failure count as unresolved,
not successful classifications. False negatives explicitly use Access as the
positive class; unresolved cases are reported separately.

## Data and replay boundary

Tenant filtering precedes state construction. Outbound state contains only text;
source IDs, tenant IDs and private metadata are omitted. This is field
allowlisting, NOT automatic redaction of secrets inside text. A future production
caller must supply already-redacted text and authenticated tenant identity.

Validated responses retain model, usage, confidence, source ID, revision and
question/policy versions. Unknown choices, malformed distributions or mismatched
types become pending errors. Confidence below 0.8 abstains; this threshold is
experimental, not calibrated correctness.

`replay` reconstructs the latest recorded revision, rejects conflicting revisions
and performs no network calls. `filter_recorded` returns matched, excluded and
unresolved IDs separately. Retain old decisions when adding reevaluations.
Caller owns append-only persistence and revision allocation; the adapter's JSON
output is not automatically written to AllSource. The separate Core
`decision_history_proof` example demonstrates actual event-store persistence.

Remaining: Prime/MMR comparison (`t-fa492a`), approved live evaluation and any
production consumer integration. No Jev relevance or speed improvement claimed.
