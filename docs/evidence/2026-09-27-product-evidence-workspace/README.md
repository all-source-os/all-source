# Product evidence workspace verification

This increment adds a default-off product workflow at
`/dashboard/tools/agent-reviews`. Customers can choose an evidence connection,
inspect a recorded run, explicitly share its pinned revision, recover saved
references, and open the same pending comparison evidence returned through MCP.
The page does not authorize or execute consequential replay or infrastructure
changes. The human-decision and complete-display beads remain open.

## Boundaries

- Product endpoints require a verified human session and current membership.
  Agent credentials are rejected. Evidence reads require the matching connection,
  audience, scope, consent, entitlement and expiry before and after source work.
- Inspection consumes one query without sharing. Sharing consumes one query and
  requires `selected-run-evidence-v1` consent over the exact revision/hash.
  Opening a human review consumes two queries. Each purpose has a separate,
  immutable Core admission identity; exact retries do not charge again.
- Saved listings contain connection-owned source references and pending receipt
  metadata. Listing is not a freshness assertion. Owner revocation remains
  available after the connection is revoked; it does not require unused quota.
- Shared evidence is revalidated through the existing deterministic comparison
  service. Product and agent outputs have the same receipt, digest and report.
  Revoked or changed sources return a metadata-only unavailable/superseded view.
- The browser uses fixed POST routes with no private data in URLs, no persistent
  report storage and no paid background polling. Failed refreshes clear the old
  view. Visibility changes and receipt expiry clear an open report. Retry state
  is held only while the current component remains mounted; reload recovers
  persisted references, not an unsaved operation key.
- Connection scope is explicit. Changing it clears consent. Only a server-issued
  evidence consent receipt adds the MCP evidence environment flag to installation
  instructions. Hosted Claude OAuth remains metadata-only.
- Query Service and web `CUSTOMER_EVIDENCE_ENABLED` remain off by default. No
  production flag, credential, registry image or deployment changed.

## Automated verification

- `web-tests.txt`: six relevant Vitest files, 46 tests, zero failures. Includes
  fresh consent on scope changes, private installation configuration, proxy CSRF,
  flag/scope boundaries, same-intent retries, saved work, failed-read clearing,
  and source revocation. Shared OAuth helpers remain covered.
- `query-service-full.txt`: six doctests and 1,137 tests, zero failures, two skipped
  and 154 excluded. This run precedes adding the opt-in manual browser fixture;
  the new production services and integration tests were already present.
- `integration.txt`: seven tests, zero failures and zero skipped, using actual frozen Core plus compiled MCP stdio/HTTP release,
  with the three new product tests and existing four transport tests. Verifies
  source inspection without disclosure, human/agent report parity, the final
  quota unit, exact retries after a Core restart, cross-owner/connection denial,
  revoked-source clearing, and revoked-connection metadata/revocation.
- Strict Credo and Dialyzer pass. Existing Dialyzer filtering is unchanged:
  seven existing findings filtered, one unused filter.
- The final isolated Next production build includes TypeScript checking and the
  existing 41-post image gate. Isolation preserves the live app's build directory.
  Initial copy-only build attempts lacked the repository TypeScript and changelog
  paths; adding those existing dependencies resolved the fixture setup failures.

## Real browser verification

An opt-in ExUnit fixture owns a temporary Core and a loopback Query Service at
`127.0.0.1:4465`; an isolated production Next build serves `127.0.0.1:4344`.
The fixture stubs only the dashboard session presentation for a synthetic user.
Evidence authentication, membership, records, metering and comparisons use the
real Query Service and Core. No request is sent to Anthropic.

Verified through Codex browser controls:

1. An explicit connection selection loads previously stored source/review
   references. Unit tests separately verify metadata-only connections are excluded.
2. Inspecting synthetic run 3 displays its revision, counts and SHA-256. Sharing
   stays disabled until the disclosure checkbox is checked. Sharing then adds a
   third persisted source.
3. Opening the prepared comparison renders baseline/candidate run IDs, revision
   7, review digest, first divergence, evidence hashes, and pass/fail attempt
   histories. The page says unapproved and no action executed.
4. Keyboard Tab reaches the evidence disclosure summary. Desktop layout at
   1280 pixels and mobile layout at 390 pixels have no document-width overflow;
   hashes wrap and comparison columns stack. Final controls use the existing
   `text-base` rank and `min-h-11` touch size.
5. Revoking the baseline source clears the open view. Reopening the comparison
   returns “Evidence unavailable” without the prior report or source details.
6. Reload recovery and final mobile layout are recorded in the browser notes.

The initial long browser run outlived the shared fixture's five-minute Core
service credential. Its connection reads returned 503. The opt-in fixture now
refreshes that synthetic service credential every two minutes; no product
credential policy changed. The UI also no longer shows an empty-state claim when
loading connections failed. The final browser run uses this corrected fixture.

The browser fixture is test infrastructure, not native Claude evaluation or a
customer outcome. Screen-reader semantic labels/statuses were inspected; no
screen-reader audio session is claimed. Screenshots were inspected in the task's
browser tool output; no independent screenshot file is represented by this report.

## Reproduction

Use the frozen Core binary whose SHA-256 is
`c4623ef235a683758d3abea8629bc0c5b4a33a9343204b2c944d77c1dcb84d64`.
The unchanged compiled MCP archive is recorded in the preceding
[`customer-evidence-bindings` evidence](../2026-09-27-customer-evidence-bindings/README.md).

Run the two integration files with `ALLSOURCE_CORE_BINARY`,
`ALLSOURCE_CUSTOMER_MCP_BINARY`, `MIX_ENV=test`, `--include integration` and seed
42017. To repeat manual browser checks, opt in with
`ALLSOURCE_HUMAN_BROWSER=true` and provide a fresh absolute
`ALLSOURCE_HUMAN_BROWSER_STOP` path to
`test/query_service_ex/integration/customer_human_browser_test.exs`. The fixture
binds loopback only and holds for at most 30 minutes. Creating the stop file ends
the test and cleans up its Core. Start the local web with both customer flags on,
the matching loopback app URL and Query Service URL. Never enable the fixture in
a production runtime.

## Remaining acceptance criteria

Consequential human accept/edit/reject authority, exact accepted-version outcomes,
generic event/restart/replay proposals, MCP App rendering and handoff, hosted
evidence consent, billing-period reset adoption, MCP notification cancellation,
native Claude evaluation and rollout remain incomplete. Full bet delivery and
the parent goal remain active. Held bets remain held.

Fly source upload and registry push remain pending destination-specific user
approval after automatic approval review rejected those actions. This increment
does not alter that pending request or the reviewed Core deployment candidate.
