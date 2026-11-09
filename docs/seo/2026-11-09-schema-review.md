# Schema evolution example: SEO review

**Status:** Prepared for review in the AllSource repository. Open [the local HTML preview](2026-11-09-schema-preview.html) in a browser; it shows the changed example, not the complete production page. Production release is unverified.

## Search intent and evidence

**Observed, 2026-11-09:** Search for event schema evolution, upcasting and missing historical fields returned [Microsoft's event-sourcing pattern](https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing) and [AWS guidance](https://docs.aws.amazon.com/prescriptive-guidance/latest/cloud-design-patterns/event-sourcing-pattern.html). Both discuss interpreting evolving event data; Microsoft describes upcasting and replay testing. These results suggest implementation and diagnostic intent. They do not establish search volume, ranking or trends.

**Observed:** treg `catalog_search` and `catalog_get` for `treg.google.keywords.trends` on 2026-11-09 list $0.0012–$0.015 per hit and possible charges for prior provider attempts. No free route was verified, and no trend call was made. No trend metric is claimed.

**Observed, 2026-11-09:** The [production guide](https://www.all-source.xyz/event-sourcing/patterns/event-schema-evolution) loads and shows a v1 event without a contract reference, followed by an instruction to resolve currency from a recorded contract version. The proposed source change supplies an illustrative version reference and says to leave currency unknown if the retained contract evidence is unavailable. It does not claim AllSource has that contract field or lookup API.

**Inferred:** Readers investigating schema evolution benefit from an example that states where the historical value comes from. Improving this existing guide avoids a competing page. Severity: moderate factual clarity; confidence: high for the observed example, subject to product-owner review of the illustration.

## Verification and limits

The HTML preview's `pre` text was compared with the proposed `example` string in `apps/web/src/lib/event-sourcing-patterns.ts` and matched exactly. It is a standalone excerpt with links to the live guide and Microsoft, not the application's rendered template. The in-app browser blocked the local `file:` URL under its URL policy, so visual rendering and link navigation were not checked. The live guide was inspected, but no production deployment of the correction was performed.

The codex-seo workflow separated observed search results and page content from inferred intent. Its Rust CLI was not run; no static score, canonical verdict, indexing result, traffic or performance metric is claimed. The existing paid-growth HOLD and qualifying renewal gate remain unchanged.

## Release blocker

`gh pr list` returned no open AllSource PRs on 2026-11-09 before the owner authorized a fresh branch. Existing product/release owner retains release decisions. After an authorized release, verify the actual public URL and rendered example, then use existing Search Console reporting for indexing evidence.
