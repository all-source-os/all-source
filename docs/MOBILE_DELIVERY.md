# AllSource complete mobile delivery

Decision date: 2026-09-21
Status: planned; implementation/release gaps remain
Authority: [delivery amendment](decisions/2026-09-21-complete-mobile-delivery.md).
Source of truth: [BET.md](BET.md). Both Android and iOS are required.

## Complete journey and fixed boundaries

Onboard and authenticate a tenant, provision hosted access, connect an existing real agent via safe key handoff, inspect ingest/recall/provenance and restart proof, buy or manage Indie, and obtain support. The mobile app controls hosted infrastructure; it does not run the agent or replace API/SDK/MCP clients.

- Economics retained: Indie £18.99/month; 14-day trial and live catalog limits unchanged.
- Existing architecture: No mobile app; retain the product stack and share domain rules.
- Privacy and scope: Keep raw events, prompts, API keys, and tenant data out of analytics; scope every read/write to the authenticated tenant. Secure credential storage and account deletion are required.
- Commercial gate: unchanged. Internal tests, founder/comped payments, downloads,
  submissions and store approval never satisfy customer qualification.
- Current release availability: unknown from this source-and-evidence audit.
  Historical submissions are retained, not represented as fresh live checks.

## Capability and evidence matrix

| Gate | Android baseline | iOS baseline | Work / evidence |
| --- | --- | --- | --- |
| Complete product journey | Missing application, bundle, billing adapter, and device proof. | Missing application, bundle, StoreKit adapter, and device proof. | t-aa85bf; t-682dee |
| Purchase and payment policy | Required flow is incomplete or unproven | Required flow is incomplete or unproven | t-60bb5b |
| Fulfilment, export/share, support | Full target needs parity proof | Full target needs parity proof | t-aa85bf; t-682dee; t-73fba7 |
| Restore, interruption, refund/revocation | Trusted lifecycle evidence incomplete | Trusted lifecycle evidence incomplete | t-60bb5b |
| Safe area, keyboard, accessibility, offline, deep links | Current signed physical-device proof incomplete | Current signed physical-device proof incomplete | t-73fba7 |
| Signed internal install | Recheck exact current build, signing and supported ABI | Signed archive and TestFlight install unverified | t-73fba7 |
| Accurate listings and declarations | Historical companion declarations need reconciliation | Complete listing and App Privacy evidence unverified | t-4e28fb |
| Review and public availability | Current status/URL unknown | Current status/URL unknown | t-4e28fb |
| Dedicated website discovery | Required Android page missing in audited source | Required iOS page missing in audited source | t-d67476 |

## Commerce acceptance

Use Google Play Billing and StoreKit for digital access, keeping Indie £18.99/month; 14-day trial and live catalog limits unchanged. Record product type, SKU, currency and entitlement unit before catalog changes. Trusted verification must bind environment, app, product and owner/outcome. Persist deduplication and entitlement issuance across retries and process restarts. Acknowledge/consume only when fulfilment is recoverable.

Prove pending/cancelled/declined, duplicate/replayed or reordered callbacks, wrong product/owner, interrupted paid delivery, reinstall/second device, refund/partial refund/revocation and relevant renewal/expiry paths. Define offline grace and revalidation; receipt recovery does not imply sync of local customer data. Prevent duplicate charges across surfaces.
Keep provider credentials server-side and tokens/customer inputs out of analytics.
Record purchase and fulfilment receipts separately, including production/test
roles and refunds. Verify current storefront policy at build and release.

## Website discovery

Target URLs, not availability claims:

- Hub: https://www.all-source.xyz/apps
- iOS: https://www.all-source.xyz/apps/ios
- Android: https://www.all-source.xyz/apps/android

Require product-specific copy and real screenshots, complete job and purchase
explanation, supported devices, accurate price, privacy/support, true release
status and verified store destinations. Link from navigation/footer/relevant
pages; verify rendered anchors, canonical metadata, sitemap, installed-app links
and uninstalled fallback. Noindex browser-handoff/privacy pages do not replace
these pages. Pending releases have no fake download badges.

## Audit sources and next-bet lesson

- [docs/BET.md](../docs/BET.md)
- [docs/LAUNCH_READINESS.md](../docs/LAUNCH_READINESS.md)
- [docs/evidence/2026-09-21-signup-repair.md](../docs/evidence/2026-09-21-signup-repair.md)

Real onboarding must survive restart and create durable tenants. Paid renewal, not trial signup, is the commercial signal.

The portfolio audit covers the ten manifest bets and maps the recurring lessons
to the future shape. Source snapshot and bead receipts are in the portfolio
`outputs/mobile-audit-2026-09-21/`. Missing evidence is unknown, not zero.

## Executable queue

| Work | Chronis bead | Reuse |
| --- | --- | --- |
| AllSource: complete Android and iOS delivery | t-e7276f | New missing work |
| AllSource: deliver complete Android product journey | t-aa85bf | New missing work |
| AllSource: deliver complete iOS product journey | t-682dee | New missing work |
| AllSource: complete verified mobile purchase and entitlement lifecycle | t-60bb5b | New missing work |
| AllSource: publish dedicated iOS and Android website pages | t-d67476 | New missing work |
| AllSource: verify signed Android and iOS complete journeys | t-73fba7 | New missing work |
| AllSource: publish and verify both complete mobile apps | t-4e28fb | New missing work |

Execution dependencies: platform work and commerce (when paid) block device QA;
device QA and dedicated discovery pages block release. Existing metadata tasks
also remain release blockers. This queues required work without changing the
portfolio's single-bet execution order.

## Release evidence required

Record per platform: source commit, bundle/package, version/build, supported ABI,
artifact hash, signing check, internal track/TestFlight installation, physical
device and OS, complete journey and accessibility proof, policy/product mapping,
listing/declarations, review state and public store URL by region. Screenshots,
configuration and upload alone cannot pass. Store credentials and legal
attestations belong to the founder in provider UI; name external blockers
without marking release done.
