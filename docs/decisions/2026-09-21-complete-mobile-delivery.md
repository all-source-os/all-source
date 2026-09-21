# Complete Android and iOS delivery decision

Date: 2026-09-21
Authority: explicit founder request to reshape all ten bets, audit missing work,
create beads first, and carry build lessons into future placements.
Contract revision: 1 (previous unversioned contract) → 2.

## Decision and boundaries

Both Android and iOS must deliver the complete AllSource outcome, with purchase
inside either app when a charge applies, plus dedicated website pages for both.
Earlier companion-only and platform-exclusion design records are historical.
Current target is in [BET.md](../BET.md); current release evidence belongs in
[MOBILE_DELIVERY.md](../MOBILE_DELIVERY.md) and [LAUNCH_READINESS.md](../LAUNCH_READINESS.md).

Economics retained: Indie £18.99/month; 14-day trial and live catalog limits unchanged.
Keep raw events, prompts, API keys, and tenant data out of analytics; scope every read/write to the authenticated tenant. Secure credential storage and account deletion are required.

## Audit evidence

- Source stack: No mobile app.
- Android: Missing application, bundle, billing adapter, and device proof.
- iOS: Missing application, bundle, StoreKit adapter, and device proof.
- Website: no complete pair of dedicated Android/iOS discovery pages found in the audited web source. Existing handoff, privacy or support pages are not equivalent.
- Current public store availability: unknown; historical submissions are not fresh provider evidence.
- Source and tracker snapshot: portfolio `outputs/mobile-audit-2026-09-21/baseline.json`.
- [docs/BET.md](../../docs/BET.md)
- [docs/LAUNCH_READINESS.md](../../docs/LAUNCH_READINESS.md)
- [docs/evidence/2026-09-21-signup-repair.md](../../docs/evidence/2026-09-21-signup-repair.md)

## Required complete journey

Onboard and authenticate a tenant, provision hosted access, connect an existing real agent via safe key handoff, inspect ingest/recall/provenance and restart proof, buy or manage Indie, and obtain support. The mobile app controls hosted infrastructure; it does not run the agent or replace API/SDK/MCP clients.

## Lessons carried forward

Real onboarding must survive restart and create durable tenants. Paid renewal, not trial signup, is the commercial signal.

Also require receipt/outcome reconciliation, sensitive-data exclusion, real
browser/device interaction, supported platform tooling, static public pages
with private-cache exclusion, and explicit release evidence. Keep customer
commercial proof separate from builds, downloads, store approval and founder tests.

## Execution

Audit precedes implementation. Reuse matching open release/validation beads;
add only missing platform, commerce, discovery and lifecycle work. Preserve
unrelated worktree changes and already claimed work. Required work and
dependencies are recorded in the mobile delivery matrix. This change creates
the contract and queue; it does not deploy apps, change live catalog prices,
accept store terms or attest readiness.
