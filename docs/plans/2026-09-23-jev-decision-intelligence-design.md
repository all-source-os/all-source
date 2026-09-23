# Optional decision intelligence for AllSource

Status: first offline proof implemented; provider integration and benchmarks remain open.
Tracker: epic `t-34d0d1`.

## Boundary

AllSource remains an event-store database. Optional typed classification belongs
in an application consumer, not Core's append path. Remote model failure must not
prevent source events from being stored. Jev is a candidate provider, not a new
database dependency or a proven improvement over current retrieval.

The first deliverable uses explicit synthetic fixture decisions. It requires no
key, network, customer content or paid calls. Subsequent work must verify the
current official API contract before writing a transport adapter.

## Work sequence

1. `t-df62c8`: persisted decision-history proof, separate-process restart and
   deterministic reducer tests.
2. `t-37ed45`: optional adapter and synthetic evaluation. Validate typed responses,
   bound timeouts/retries, redact selected fields, record pending/error/abstention
   outcomes and make outbound requests explicitly opt-in.
3. `t-f26ebe`: tenant-safe semantic filtering of stored classifications. Tenant
   scope must precede field selection and any inference. Missing classifications
   remain visible; they must not silently disappear from results.
4. `t-fa492a`: optional Prime candidate reranking against existing local scoring
   and MMR. Preserve local fallback on timeout, error or invalid responses.

The last two tasks depend on the adapter/evaluation contract. The existing
`t-e3d99f` agent-run evidence epic is related but concerns a different surface.

## Event semantics

Source → recorded classification → human correction → explicit reevaluation.
Classifications reference source sequence plus question/policy version and
provider/model identity. Corrections reference a prior classification. Replaying
history never reruns a model. Human overrides remain effective during machine
reevaluation; revoking an override would need a separate explicit event.

The example uses one synthetic entity and one writer. Its local application
sequence is not a distributed ordering or deduplication guarantee. Production
consumers still need tenant-scoped identities, idempotency and authorization.

## Evidence gate

Use held-out labels to compare rules, existing retrieval and optional Jev.
Report errors, false negatives, abstention coverage, p95 latency and cost per
event. Confidence is not correctness. No provider superiority or production
readiness claim is justified by fixture tests. Live evaluation requires explicit
approval; no customer content should be sent as part of this experiment.

## Verification

Core Makefile provides broad `make check` / `make quality-gates` targets. For this
example-only slice, use scoped Cargo tests, run the example, and rustfmt check:

```console
RUSTC_WRAPPER= cargo test -p allsource-core --no-default-features --features embedded --example decision_history_proof
RUSTC_WRAPPER= cargo run -p allsource-core --no-default-features --features embedded --example decision_history_proof
rustfmt --edition 2021 --check apps/core/examples/decision_history_proof.rs
```

## References

- `apps/core/examples/decision_history_proof.rs`
- `apps/core/examples/event_store_restart_proof.rs`
- `apps/core/src/prime/facade.rs` (current recall/MMR implementation)
- [TypeSafe introduction](https://docs.typesafe.ai/introduction)
- [Confidence](https://docs.typesafe.ai/confidence)
- [JevQL](https://github.com/kylemclaren/jevql)

References informed the proposal; this fixture does not implement their API.
