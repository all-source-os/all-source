# Customer replay approval gate

Status: default-off implementation. This document does not authorize deployment,
flag activation, customer consent or an actual Claude host connection.

## Configuration

Prerequisites are the existing durable Core conditional configuration/query
admission candidate, configured leader access, verified product sessions, current
Control Plane team membership, MCP entitlement and an enabled curated projection.

| Surface | Additional configuration |
| --- | --- |
| Query Service | `CUSTOMER_REPLAY_ENABLED=true`, existing review/connection/evidence flags, `CUSTOMER_HUMAN_ACTION_SECRET` |
| Web | `CUSTOMER_REPLAY_ENABLED=true`, existing connection/evidence flags, same action secret, canonical `NEXT_PUBLIC_APP_URL`, existing `QUERY_SERVICE_URL` |
| Existing customer MCP release | `ALLSOURCE_CUSTOMER_REPLAY_REVIEW=true`, existing customer/evidence profile flags and private connection file |

The action secret must be a distinct random server secret of at least 32 bytes,
never the JWT signing secret. Provision through the normal secret manager after
deployment approval. Never place it in `NEXT_PUBLIC_*`, browser code, connection
files or customer skills. Missing or mismatched secrets deny the product relay.
Both services must be updated together for rotation; expired relay proofs remain
invalid. Keep all flags off until remaining release and actual host gates clear.

Customers must explicitly create a `review-replay-v3` connection. Existing v1/v2
consents do not gain replay access. Hosted Claude OAuth remains metadata-only;
the tested replay binding is the existing compiled stdio connector. The setup
screen adds both evidence and replay flags only for an issued v3 receipt.

## Product workflow

Open authenticated `/dashboard/tools/agent-reviews`, select the connection,
inspect an enabled projection and explicitly share the bounded analysis. A
product-generated proposal contains one `replay_analysis` reference. Claude can
validate, prepare and read it; the product can also prepare it. No agent tool
approves, rejects, edits or executes the plan.

The review displays exact version/hash, expiry, analysis facts, unknowns and the
effect on the selected projection. Its owner must currently be an administrator
to edit or decide. Relevant edits increment the version. Approval binds that
version to one durable replay operation. Rejection preserves accepted state.
Source revocation clears open product views and denies subsequent evidence reads.

Analysis inspects at most 1,000 events and stores hashes and counts only. Unknown
total-count provenance, ordering, archive completeness, restart proof and run
comparison remain visible. The authorized rebuild uses retained history at
dispatch with live catch-up. Freshness checks happen before approval; later
arrivals are within this expressly reviewed scope, not a frozen-history promise.

## Recovery and failure

- `pending`: no human decision; an enabled, current plan may be reviewed.
- `superseded`: analysis facts, sampled bytes or reducer revision changed. Inspect
  and share current evidence, then replace the pending plan or prepare a new one.
- `expired` / denied source: no further approval or evidence disclosure. Re-select
  evidence with valid consent. Expiry does not undo an already completed rebuild.
- `approved` + `not_started`: the decision persisted but no dispatch record exists.
  Only an explicit product retry of that exact receipt may dispatch its original
  operation after current authority, expiry, enablement and freshness checks.
- `unknown`: a reservation exists but dispatch/completion cannot be proved. Read
  again to recover durable progress. Never create a new operation as an automatic
  retry; the old worker may have run.
- `running`, `completed`, `failed`, `cancelled`: report the recorded result. A
  completed rebuild is not proof of complete source history or business success.

Lost acknowledgements may hide committed state. Keep the original idempotency,
request and decision IDs with unchanged inputs. Browser retry state survives
uncertain responses while mounted; after reload, recover saved reviews/results.
Do not interpret a 503 as proof that nothing happened. Revoked roles/connections
remain denied even when recovering an existing operation.

## Bounds and retention

Replay samples are leader-only, bounded to 2 MiB and six seconds. Product request
bodies are bounded to 16 KiB and relay proofs to 30 seconds. Existing workflow
admission bounds concurrent work. Inspect/share/prepare/pending read/pre-approval
checks each cost one admitted query; exact retries reuse their charge. Execution
uses the existing replay engine, never an agent preparation side effect.

Core's current review registry holds at most 32 records and 60,000 encoded bytes.
Writes may remove other current records more than 24 hours past expiry. This is
current-view pruning, **not deletion of historical Core events**. Minimal owner,
plan, source hashes, approval and execution metadata remain subject to Core's
existing backup/retention policy; raw sampled payloads are never stored in review
records. Do not promise deletion or redact history by mutating event logs.

Disable the replay flag to stop new product/MCP gate requests; it does not cancel
an already started replay. Use the existing operator cancellation workflow where
authorized. Preserve Core review and replay records during rollback so operation
identity cannot be reused. Never backfill an approval from chat or model output.
