# Agent-run evidence: initial read implementation

Owner: existing Chronis epic `t-e3d99f`. This is the shared run substrate required
by the customer-review contract, not another comparison feature. It is internal
Query Service functionality; no new public route, MCP binding, SDK wrapper or
customer data consent is enabled by this implementation.

## Stored contract

One run has a lower-case UUID. Its entity is
`agent-run-v1-<SHA-256(tenant_id)>-<run_id>`. The authenticated calling boundary
must supply the tenant. The prefix avoids incidental overlap with existing
entities and other tenants; it is not an authorization credential. Every read
also filters by tenant and verifies the tenant/entity/run on each returned event.

Event type is `agent_run.v1.<kind>`. Payload has exactly these fields:

| Fields | Constraint |
| --- | --- |
| `schema_version` | Integer 1 |
| `run_id` | Lower-case UUID |
| `kind` | Closed lifecycle vocabulary below |
| `change_id`, `change_number` | UUID and positive integer for change/attempt events; null for run events |
| `attempt_id` | UUID only for attempt events; otherwise null |
| `causation_id` | Previous acknowledged Core event UUID; null only on first event |
| `agent_sha256`, `model_sha256`, `prompt_sha256` | Three lower-case SHA-256 identifiers on `run.started`; null thereafter |
| `evidence_sha256` | Nullable SHA-256; required for proposal, recorded approval, test, reversion and reconciliation |
| `outcome` | `pass`/`fail` for tests, `succeeded`/`failed` for reconciliation; otherwise null |

Lifecycle kinds: `run.started`, `run.completed`, `capture_gap`,
`change.proposed`, `change.approved`, `change.abandoned`, `attempt.started`,
`attempt.tested`, `attempt.failed`, `attempt.accepted`, `attempt.reverted`,
`attempt.unknown`, `attempt.reconciled`.

No raw prompt, summary, file content, tool arguments, actor email or credential
field is accepted. Generic Core ingestion remains compatible and can store other
data; this typed reader rejects payloads outside this contract. Envelope metadata
is omitted. Hashes describe supplied evidence; they do not verify the external
test, model identity, person, signature or outcome.

## Ordering and recovery

The real HTTP reproduction returned acknowledgement versions `[1,2,3]` while
stored events returned `[1,1,1]`. Core's conditional append now stamps its assigned
version before WAL persistence and uses the same event for storage, subscribers
and projections. It hydrates cold tenant history before checking the expected
version, so a write immediately after Parquet-only startup cannot start at zero.
The change applies to the existing HTTP single/batch append path, with or without
`expected_version`; it does not rewrite old events or change the separate generic
embedded `ingest`/batch APIs.

The read model requires a complete retained run of at most 1,000 events,
unique event IDs, versions exactly `1..N`, one initial `run.started`, and an
unbroken causation chain. Input order and timestamps do not determine sequence.
Missing, duplicate or legacy constant versions return `order_uncertain`.
Conflicting lifecycle transitions fail closed. Historical gaps are never repaired
by timestamp sorting or renumbering.

Existing Core counters are keyed by entity, not a tenant/entity tuple. Broader
counter isolation, cache-eviction races and retention/high-water-mark semantics
are not solved here. The new namespaced run key and strict sequence/causation
checks prevent this reader from treating ambiguous history as verified order.
Do not extend the ordering claim to arbitrary legacy streams. A data-center
failover test has not run.

## Read model and comparison

`AgentRunEvidence.read/2` uses the source port and returns an ordered timeline,
change/attempt states, revision and canonical digest. Unfinished actions and
explicit capture gaps stay unknown. Recorded approval never establishes product
human authority. `restart_proof` and `approval_authority` explicitly remain
`not_established`; a local restart test is not a customer-specific attestation.

`page/4` slices only the validated revision, with at most 100 events per page.
The caller supplies the exact digest; a changed digest refuses continuation.
This is pagination over a bounded complete read, not an unbounded scalable scan.

`compare/3` reads two runs from the same tenant and aligns numbered changes.
It compares proposal evidence hashes, outcomes and test fingerprints, reports
the first changed/missing change, descriptor changes, unknowns and both source
digests. It never calls tools or projection rebuilds. Runs are independently
observed revisions, not one atomic cross-run snapshot. Equal partial history is
`inconclusive`; equal complete records are `same_recorded_evidence`, not proof
of equal real-world behavior.

Retry decisions are advisory only. Pending/unknown outcomes prevent retry;
confirmed failures count toward a configurable cap (default two); completed,
accepted or abandoned runs/changes stop. No SDK wrapper enforces this yet, and
the decision grants no authority to execute an external action.

## Resource and privacy boundary

The existing Core leader is queried once using fixed tenant/entity filters and
a 1,001-event limit to detect oversized runs. HTTP/1 streaming retains at most
2 MiB before JSON decoding, with a six-second overall deadline and shorter
connection/receive timeouts. Redirects and retries are disabled. Partial response
envelopes, malformed JSON, unavailable Core and schema violations return fixed
errors. These are internal source references, not URL-based customer handoffs.

No second datastore, mutable proposal store or model call is added. Core remains
durable source of truth. No customer MCP consent is expanded: current
`review-metadata-v1` still permits eligibility and validation only.

## Remaining work in the existing epic

Authenticated REST/SDK read/write boundaries, typed append/idempotency and
write-before-execute wrappers; language parity; current per-tenant entitlement
and query metering; normal product UI; actual source handles and their consent;
review persistence and product human gate; complete native/web host journeys;
release/deployment and customer-outcome evidence. Keep these open. See
[verification](../evidence/2026-09-27-agent-run-evidence/README.md).
