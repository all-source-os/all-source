# Customer source references and pending comparison reviews

Status: internal implementation under existing `t-7ce02d` and `t-7b4430`.
This extends the [review contract](2026-09-26-customer-agent-contract.md) and
reuses the [run evidence domain](2026-09-27-agent-run-evidence-contract.md).
No source-selection route, MCP preparation binding, product review page, approval
operation or production activation is added by this increment.

## Consent and current authority

Existing `review-metadata-v1` grants remain eligibility/validation only. A new,
separately accepted `review-evidence-v2` contract permits those existing fields
plus `selected_run_metadata`, `comparison_evidence`, and `pending_review_status`.
Its closed operation vocabulary adds `prepare_proposal`, `read_review`, and
`read_result`. V1 receipts cannot authorize these operations. Existing consent UI
and remote OAuth flow still request v1; their discovery remains two tools.
The new consent version is not automatically assigned to existing connections.

Source selection additionally requires `selected-run-evidence-v1` acceptance
for an exact run ID, revision and digest. The eventual product UI must describe
the actual fields: run/change/attempt IDs, server versions and timestamps,
evidence and agent/model/prompt hashes, recorded transitions, test outcomes,
comparison differences and unknowns. No raw prompt, source, arguments, arbitrary
query or restart attestation is added. Host processing and transcript retention
need the existing disclosure and real-host verification before activation.

`CustomerEvidenceSources.share/4` accepts an actor already authenticated by the
product transport. It verifies that actor owns the selected active connection,
has current stored membership and persisted MCP entitlement, and accepted the
evidence consent. It reads the actual typed run, checks its requested pin,
persists a scoped reference, and checks eligibility again before returning it.
This is an internal service, not proof that a browser human made a selection.
No public handler accepts caller-supplied actor maps.

`CustomerEvidenceReview.prepare/4` and `read/6` verify the actual scoped token,
binding, consent version, current member/entitlement and revocation before and
after work. Raw run IDs are not accepted by preparation. Each selected reference
must also match tenant, subject, client, resource and exact grant ID. Another
connection belonging to the same subject has no implicit access. Human action
authority is never conferred by connection consent or these records.

## Source and proposal records

Sources contain only owner/host/grant binding, opaque ID, run locator, revision,
digest, request fingerprint and validity interval. Access expires within one hour
or earlier connection/entitlement expiry. Resolution repeats owner, source pin,
expiry and deny-marker checks before and after the actual bounded Core read.
Generic stored payloads still pass the strict run schema before comparison.

Initial preparation accepts exactly a typed proposal, `expected_revision: 0`,
and UUID `idempotency_key`. It currently resolves exactly two `run_evidence`
sources for `run_comparison`; timeline/event-range, restart-evidence and projection
replay preparation remain explicitly unsupported, not silently substituted.
Runs are independently observed pinned revisions, not an atomic cross-run read.

Pending records contain version 1, owner binding, the typed opaque source
references, request fingerprint, exact content digest, view contract
`run-comparison-v1`, authority contract `allsource-operator-review-v1`, creation
and expiry. Their digest binds this entire record and the deterministic comparison
report. The report is rebuilt from currently authorized sources rather than copied
into immutable proposal metadata. No approved state or execution receipt can be
constructed by the pending-record type.

Same-owner idempotency retries return the original record, preserving its digest
and expiry. Changed inputs under the same key fail. Unknown writes may already
have committed; retries recover by the same key. Concurrent writers use Core's
conditional config revision, with at most four conflict retries. No duplicate
accepted result is created; accepted-result execution itself is not implemented.
Editing an existing proposal and its expected-version flow remain open.

Reads return a current pending view, or an explicit expired, superseded or
unavailable receipt without source contents. Deleted records deny access.
`read_result` reports `result_available: false` for pending work; it cannot mint a
delivered outcome. Every response preserves `approved: false` and `execution:
none`. Unknown source information remains unknown.

## Durability, limits and deletion

The existing admin-only Core config API stores one bounded tenant workspace.
Source IDs and review IDs derive from a tenant/subject/host/grant-bound operation
fingerprint; they are not bearer capabilities. There is no second datastore or
process-local-only draft map. HTTP always uses the configured leader, service
authority, fixed paths, no redirects and no transport retries.

Operational bounds: 64 source issuances and 32 pending reviews per rolling day
per tenant; 60,000 bytes per stored workspace; 65,536 streamed response bytes;
48,000 bytes per comparison report; six seconds per storage request; 20 seconds
for a complete share/prepare/read call. These are abuse bounds, not new prices or
commercial entitlements. Oversized or partial data is refused, never sampled into
a falsely complete review. Existing paid/trial catalog is unchanged.

Independent, conditionally created deny markers block source/review retrieval
even if an older workspace registry is restored. This protects against a stale
workspace write, not rollback of the entire security datastore. Deletion is access
revocation; immutable WAL/Parquet history is not advertised as erased. Old records
are pruned from the current workspace on later insertions after a day, which does
not itself erase audit history. The production audit-retention/cleanup policy and
owner-facing deletion route remain required. Idempotency is bounded to retained
metadata; no indefinite replay or erasure guarantee is made.

## Remaining delivery work

Before any source route/tool opens: durable query metering and request/concurrency
admission; product session/CSRF source-selection and consent UI; preparation,
status and result transport bindings; authenticated product display; owner-facing
deletion; actual human decision/receipt and exact replay reconciliation;
event-range/restart/projection evidence; complete Claude skill and native/web host
proof; production rollout and existing commercial gates. Current membership,
billing and registry reads are sequential, not one atomic transaction.

[Verification and exact limits](../evidence/2026-09-27-customer-evidence-reviews/README.md)
remain separate from customer consent or outcome evidence. All dependent beads
remain open; held bets remain held.
