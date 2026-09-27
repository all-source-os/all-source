# Product evidence selection and review

Extend the existing Agent connections settings with an explicit evidence-consent
choice and add `/dashboard/tools/agent-reviews` using existing UI components,
semantic colors, rem/Tailwind spacing and the established 16px body/20px section
heading scale. No new theme tokens or alternate comparison domain are needed.

## Journey

1. The human chooses metadata-only or selected-evidence access. Changing scope
   clears acceptance. Evidence consent discloses run metadata, comparison evidence,
   pending status and host processing; it cannot approve or execute anything.
2. In Agent reviews, the owner chooses a connection and enters a recorded run ID.
   A metered product-only inspection returns the current digest, revision and
   bounded summary. It does not share the run with the agent.
3. Explicit per-source acceptance shares the exact inspected pin. Changed history
   refuses sharing. Owners can revoke a source without granting agent authority.
4. Select two shared pins and copy the typed comparison proposal for the configured
   assistant. Saved source references and pending reviews recover from Core;
   browser storage never holds credentials, reports or a second source of truth.
5. Open a saved pending comparison. The human-session endpoint reuses the exact
   comparison service and immutable digest contract, with a separate metering
   purpose. Display baseline/candidate provenance, differences, unknowns, expiry
   and explicit unapproved/unexecuted state. Failed refresh removes old evidence.

A dedicated page keeps connection setup focused and gives later consequential
review actions a stable product surface. Embedding all evidence inside settings
would obscure the review journey; introducing a separate app would duplicate
identity and data boundaries. Those alternatives are rejected.

## Boundaries and recovery

The Next proxy accepts only cookie-authenticated same-origin POSTs to a fixed
operation allowlist; Query Service verifies JWT signature and current membership.
Agent tokens cannot call human endpoints. Minimal owned record summaries remain
available for recovery/revocation without unused quota; event evidence requires
current grant, evidence scope, entitlement and canonical query admission.
Inspection costs one query, source sharing one, and an explicit human review read
two. Exact retry identities are retained while a request can be retried; changes
to intent invalidate the pending retry. No automatic paid polling occurs.

All authority-bearing record IDs and payloads stay in POST bodies. The page URL
is fixed, the private subtree blocks capture, and reports never enter analytics
or persistent browser storage. Metadata listings label pins as saved, not fresh
source validation. Current review retrieval establishes freshness.

This completes a comparison selection/display path, not the whole bet. Actual
consequential replay/infrastructure proposals, accept/edit/reject authority,
MCP cancellation, remote evidence-consent UI, native host proof and billing reset
adoption remain required in existing tasks. No approval control is substituted
for acknowledging a read-only report. All new product features remain gated off.

## Verification

Use real Core with synthetic histories for source inspection, sharing, saved
listing, human/agent parity, revoke, final-unit retries and restart recovery.
Test foreign owner, altered digest/version, forged sessions, agent bearers,
changed sources, current membership and expired connections. Verify the actual
Next proxy and product page in a local desktop/mobile browser, including keyboard
flow, readable typography, loading/empty/error/stale states and private output.
