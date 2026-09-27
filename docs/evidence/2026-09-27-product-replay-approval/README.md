# Product replay approval verification

This increment implements default-off product approval of one curated tenant
projection rebuild. It extends the existing evidence workspace and five MCP
tools; no second MCP server, datastore or ordinary ingestion approval requirement
is introduced. It follows the 26 September customer contract's bounded replay
analysis, not a new full-history snapshot protocol.

## Implementation

- Explicit `review-replay-v3` consent permits selected replay analysis, plan and
  result metadata. V1/v2 grants do not gain that scope. Each analysis is separately
  selected in the product; raw sample payloads are never returned to the host.
- The product shows counts from at most 1,000 events, source/reducer hashes,
  analysis time and five explicit unknowns. It reviews retained-history replay at
  dispatch plus live catch-up. Source facts and reducer revision are rechecked;
  elapsed inspection time alone does not supersede the facts.
- Query Service requires a verified product session and an operation/body/session
  bound relay proof signed by a separate server secret. A generic user bearer,
  agent credential, approved flag or OAuth consent cannot decide. Membership and
  enabled projections are checked against current leader-backed authority.
- Core CAS stores exact version/digest, owner/admin, action, expiry and one replay
  operation. Competing edits/decisions cannot bind another plan to that receipt.
  Lost approval acknowledgements recover the stored receipt. Durable dispatch
  reservation permits only one replay; uncertain execution stays unknown.
- The browser supports inspection, explicit sharing, proposal handoff, prepare,
  edit, approve/reject, saved-result recovery and source revocation. Failed reads,
  expiry, visibility changes and source revocation clear open evidence. Result
  headings receive keyboard focus. All action controls use existing base type and
  44-pixel minimum touch targets; long hashes wrap.
- Existing MCP preparation/read tools accept replay plans only with the extra
  replay flag and v3 grant. Closed input/output schemas distinguish review state,
  approval receipt and execution. MCP never approves, edits or dispatches.

## Automated evidence

| Check | Result |
| --- | --- |
| `integration.txt` | 21 tests, zero failures; actual frozen Core plus rebuilt compiled MCP release |
| `query-full.txt` | Six doctests, 1,137 tests, zero failures, two skipped, 171 excluded |
| `mcp-full.txt` | 654 tests, zero failures, five excluded; completed in 181.5 seconds |
| `web-tests.txt` | Six Vitest files, 49 tests, zero failures |
| `query-credo.txt` | Strict Credo, 334 source files, clean |
| `query-dialyzer.txt` | Passed; existing seven filtered findings and one unused filter unchanged |
| `mcp-credo.txt` | Repository's configured strict checks, 51 files, clean |
| `tenant-isolation.txt` | Both isolation and Core/QS responsibility checks pass |
| `web-build.txt` | Isolated production Next webpack build including TypeScript passes; existing 41-post image gate also passes |

The full Query Service regression predates the last output-metadata addition and
concurrent-edit test. Final integration, compile, Credo and Dialyzer cover those
changes. The final browser build includes the focus and cross-panel revocation
fixes. Initial isolated-copy build attempts lacked relative TypeScript/changelog
inputs; supplying those repository dependencies fixed setup, not product code.

Nine new real-Core gate cases cover actual execution/reload, conflicting decision
IDs, agent/generic JWT/altered/expired proof denial, malicious source content,
changed sample facts, live role and projection disablement, revoked source/grant,
v2 consent denial, edited versions, lost approval acknowledgement across restart,
competing decisions, concurrent edit versus approval, cross-owner/tenant denial
and expiry. Existing tracked-replay cases cover cancellation, failure, lost
dispatch/completion acknowledgements and restart without a duplicate fold.

The compiled stdio test discovers the existing five tools, prepares and reads the
same pending review, denies forged authority and reads the exact product-approved
result. Turning the Query Service replay flag off denies subsequent replay reads.
This is actual compiled transport evidence, **not** a Claude account/host outcome.

## Actual browser evidence

Opt-in ExUnit fixture: owned temporary Core and actual Query Service on loopback
4467; isolated production Next on 4346. Only dashboard identity presentation is
synthetic; grants, source selection, membership, metering, approval and replay use
the real services. No Anthropic request or customer data was involved.

Verified through Codex browser controls:

1. Selecting the v3 connection recovered its previously prepared review.
2. Inspection displayed one sampled event/entity, reported total one, current
   entity count one, analysis time and unknowns. Sharing remained disabled until
   the explicit disclosure checkbox was checked.
3. Sharing enabled proposal handoff and preparation. The new review displayed
   version one, bounded facts, exact hash disclosure, expiry and rebuild effect.
4. Rejecting that review persisted rejection and showed only its minimal receipt
   in the immediate response. Accepted projection state stayed unchanged.
5. Keyboard Enter on another pending review's product approval button produced
   `approved` plus `running`. Page reload and a fresh result read recovered
   `completed`, one processed event and the same tracked execution ID.
6. On the final build, opening a result focused its heading. Revoking that source
   removed the open report; reopening was denied with no retained evidence.
7. Final 390-pixel mobile and 1,280-pixel desktop documents matched viewport width
   without horizontal overflow. Mobile hashes and actor names wrapped; decision
   and execution stayed separate. Temporary viewport override was reset.

Synthetic approval verifies the implementation, not a customer's human decision.

## CI formatting follow-up

The initial commit `4048948da897b39dcb96368db2756699348a495a` failed the
Query Service formatting gate in CI run `36353591012`: Elixir 1.18 wrapped two
header expressions differently from the local Elixir 1.19 formatter. Reusing
the existing session headers keeps the test cases unchanged and the expressions
short enough for both formatters. Local `mix format --check-formatted` passes;
the nine real-Core replay review tests pass again with zero failures. The source
manifest now pins this test-only follow-up; the original full-suite logs above
remain evidence for the initial implementation. CI confirmation of the follow-up
is tracked separately from those local results.

## Release limits

No production flag, credential, Fly source upload, registry image or deployment
changed. Native Claude processing approval, actual customer installation/host
verification and destination-specific deployment approval remain separate gates.
Hosted Claude OAuth remains metadata-only. Event timelines and restart-proof
delivery remain outside this increment. Broad substrate/display/host tasks stay
open; held product bets remain held.

See [configuration and recovery](../../runbooks/CUSTOMER_REPLAY_APPROVAL.md)
and [design](../../plans/2026-09-27-product-replay-approval-design.md). The source
manifest pins changed application/test/skill files; runtime artifact hashes pin
the frozen Core and compiled MCP beams used by verification.
