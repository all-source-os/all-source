# Chat delivery readiness

Product: all-source. Contract v3; chat schema 1. Activation: **queued**.

## Customer job

- Trigger: An operator investigates an event timeline, restart or replay result inside a conversation.
- Advantage: Authoritative event provenance, tenant-scoped retrieval and executable restart/replay evidence.
- Result: Read-only event timeline with source evidence and restart status.
- Interaction: Filter a permitted time window and inspect an event or replay result without executing a write.
- Data boundary: Tenant authorization on every retrieval. No raw secrets, cross-tenant history, arbitrary query execution or model-authorized writes. Evaluate existing MCP interfaces before adding another server.
- Commerce mode: sample-only. No digital checkout or upsell in chat. Existing prices and qualified-outcome gate stay in [BET.md](BET.md).

## Independent evidence

| Gate | Status | Evidence or named blocker |
|---|---|---|
| Shared domain implementation | not-started | Queued or deferred; no product adapter claimed |
| MCP protocol and text fallback | not-tested | Record dated executable test evidence; no inference from build success |
| Browser interaction/accessibility | not-tested | Record keyboard, narrow viewport, error and interaction proof |
| Actual ChatGPT host | not-tested | A compatible connected host and reviewed endpoint are not yet evidenced |
| Actual Claude host | not-tested | A compatible connected host and actual UI interaction are not yet evidenced |
| Customer authentication/entitlement | blocked | Product-specific paid-account eligibility, trusted verification and recovery are unresolved |
| Refund/revocation | blocked | Must reconcile trusted status before paid use; no bearer checkout tokens in chat |
| Production endpoint/privacy | blocked | Pilot is local; customer processing disclosure and access boundary need release proof |
| Host publication/review | not-tested | No marketplace availability claim |
| Qualified customer outcome | unknown | Reconcile canonical product evidence; sample use never counts |

## Work queue

Strong later fit using existing MCP work, but no second active pilot. Preserve paid-use and renewal evidence requirements. Root Beads epic `t-ddf1c3`: placement `t-ebe5d7`, adapters `t-7be3df`, amendments `t-f5a1f0`, Sponsor pilot `t-0d6ca5`, private portfolio `t-9744bc`. This record does not activate a second bet.
