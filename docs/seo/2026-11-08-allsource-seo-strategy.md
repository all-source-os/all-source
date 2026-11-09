# AllSource SEO strategy

**Decision:** Make AllSource discoverable for developers evaluating event sourcing, then help them run a small, inspectable evaluation. Use agent memory as a separate application cluster. Improve existing pages before creating more. Preserve the hosted paid-growth HOLD and existing customer/renewal gate in `docs/IMPACT_PLAN.md` and `docs/BET.md`; search traffic is not paid validation.

## Evidence and limits

- **Observed, 2026-11-08:** The browser returned a 404 for `https://www.all-source.xyz/blog/agent-memory-after-compaction`, although the article source exists on the fetched main branch. The cause and the deployed revision are unknown. Establish the active web deployment path with its owner before changing production.
- **Observed, historical:** `docs/reports/2026-10-20-seo-evidence-baseline.md` reports a September 2026 SpyFu US estimate of 12 ranked keywords and two organic clicks per month. Its backlink vendors disagreed: Moz observed three referring domains on October 16; SE Ranking listed seven. These are third-party estimates, not current Search Console counts or proof of causation.
- **Observed, historical:** `docs/seo/2026-10-01-allsource-wolventech-keyword-demand.md` proposes event-store, CQRS and replay topics, with agent memory as a separate cluster. Its October keyword volumes are historical provider estimates. Generic `allsource` queries cannot be attributed to this product without further evidence.
- **Observed, 2026-11-08:** Public searches for event-store comparisons and schema evolution return [Confluent's event-store explanation](https://developer.confluent.io/patterns/event-storage/event-store/) and [Microsoft's event-sourcing guidance](https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing). This is evidence of comparison and implementation intent, not measured demand or a ranking opportunity. Google recommends [people-first content](https://developers.google.com/search/docs/fundamentals/creating-helpful-content) with original value.
- **Setup required:** Current first-party impressions, clicks, indexed URLs and activation by landing page were not read. The 2026-10-20 report's API setup status is historical; check the current owner and existing connector before requesting access or drawing conclusions. Revenue and qualifying paid outcomes remain unknown.

## Sequence

### 1. Restore delivery and measurement

The release owner should identify the production artifact and deployment path, resolve the guide's 404, and verify its rendered title, content and internal links at the live URL. Check the deployed introductory article for the merged Kafka and database-versioning corrections. Do not assume a merge deployed the site.

Use the existing Search Console property and analytics pipeline, if available. Capture property, country, device, query, landing page, date range and extraction date. Record indexed status through URL Inspection for the guide and the core landing page. Mark missing sources as unknown; do not create another collector.

### 2. Concentrate intent on existing pages

| Reader question | Primary destination | Useful improvement |
| --- | --- | --- |
| What is an event store and should I use one? | `/platform/event-sourcing` and `/what-is-an-event-store` | Give each page a distinct job: product evaluation versus concept. Explain fit, tradeoffs and a reproducible first step; link between them. Verify current live copy before editing. |
| How do I implement replay, CQRS or schema evolution? | `/event-sourcing/patterns`, `/blog/cqrs-vs-event-sourcing`, `/event-replay-validation-checklist` | Maintain worked examples and failure cases; connect each guide to the relevant checklist and evaluation path. Validate links and product claims against current contracts. |
| How can an agent recover trustworthy memory? | `/solutions/agent-memory` and `/blog/agent-memory-after-compaction` | Keep memory separate from the generic event-store category. Explain scope, provenance, correction and restart boundaries; distinguish synthetic fixtures from verified customer results. |

The schema-evolution example correction accompanies `docs/seo/2026-11-08-schema-evolution-evidence.md`. It adds the contract reference its example needs and treats missing evidence as unknown. The correction requires product-owner review and live verification after release.

### 3. Build useful proof that can earn references

Maintain one reproducible synthetic replay fixture: inputs, expected latest and historical state, exact restart boundary, duplicate handling, commands and known limits. Link it from the relevant guides and product evaluation page. Compare an ordinary database or existing framework checkpoint fairly, where evidence supports it. Do not promise compliance, benchmark superiority or recovery under conditions not tested.

Make the fixture, documentation and reference implementation easy for independent engineers to cite. Record external references only when observed. Outreach, submissions and paid distribution require separate authority; do not buy links or count owned-domain links as independent endorsements.

## Measurement and decisions

Compare equivalent first-party reporting windows after the delivery path is verified. Separate brand and non-brand queries and label country and device. Track impressions, clicks and landing pages as discovery diagnostics; then track privacy-safe progression to documentation, starting a real workload, successful replay/restart and qualifying paid renewal where existing measurement permits. Do not infer customer value or revenue from visits or trial signups.

Decide what to improve using observed query-to-page mismatch, reproducible reader failures and verified delivery status. A new article needs a distinct reader question, evidence and a useful example not already covered. No numerical growth targets are set until a current baseline exists.

## Ownership and verification

Existing product/release owner controls deployment and production verification. SEO owner prepares evidence and copy in AllSource, uses a live authorized feature branch, and checks rendered output after release. Recheck page responses, canonical declarations, sitemap inclusion and internal links; treat these as signals, not proof of indexing. Use first-party URL Inspection for indexed status. Keep user and customer payloads out of fixtures and reports.
