# AllSource and Wolven Tech search-content focus

Date: 2026-10-01. Verification scope: local production builds and browser checks. Production release verification is separate.

## Evidence and scope

The [keyword demand report](2026-10-01-allsource-wolventech-keyword-demand.md) supplies the treg/DataForSEO evidence. Google Ads estimates cover English searches in the US and UK; the latest reported month is August 2026. Values below are rounded average monthly estimates, not exact queries, site traffic or forecasts. Close variants must not be added together.

Existing September 2 AllSource SEO caches were inspected for context. Their scores, deployment findings and analytics descriptions were not reused as current observations. The changes below use current repository content and local rendering evidence.

## One intent per destination

| Site and destination | Reader's decision | Supporting demand, US / UK |
| --- | --- | --- |
| AllSource `/platform/event-sourcing` | Evaluate an event sourcing database | `event sourcing database`: 70 / 20; `event store database`: 50 / 20 |
| AllSource `/blog/event-store-vs-database` | Choose current-state or event-history storage | Related category intent; no invented volume for this exact title |
| AllSource `/blog/cqrs-vs-event-sourcing` | Decide whether to separate models, retain events, or do both | `event sourcing vs cqrs`: 30 / 10; `cqrs pattern`: 1,300 / 390 |
| Wolven Tech `/services/rust-consulting` | Engage one named Rust consultant for a scoped outcome | `hire rust developers`: 110 / 20; `software architecture review`: 20 / 10 |
| Wolven Tech `/services/software-technical-due-diligence` | Evaluate software engineering risk with evidence | `software technical due diligence`: 10 / 10; `technical due diligence checklist`: 40 / 10 |

The inferred opportunity is better intent matching, not guaranteed ranking growth. CQRS has an NHS-related meaning in UK results. “Rust Consulting” also names a legal-settlement business. Software and Rust-language qualifiers remain explicit. Wolven Tech stays a single-engineer advisory practice, not a staffing agency or fractional CTO offer.

## AllSource content changes

- Replace the database comparison's false dichotomy with the distinction between authoritative current state and retained domain changes. Explain that an event store is a database and PostgreSQL can implement event storage.
- Remove unsubstantiated sub-100ms historical-query, compression and storage-cost promises from that article.
- Add a worked subscription example to the CQRS guide, distinguishing commands from accepted facts, duplicate handling, projection lag and recovery evidence. Code blocks are explicitly illustrative rather than invented SDK calls.
- Link both articles from the product page, and the CQRS guide from the pattern hub. Existing blog discovery feeds the sitemap and page metadata.
- Keep agent memory as a supported downstream use of the event-store category. Preserve the existing memory cluster, product branding and logo assets.
- Contain comparison-table columns in the existing article renderer after local browser evidence showed 9 pixels of mobile overflow.

The new article includes a subscription-flow diagram, with PNG output for article and social metadata and an editable SVG source. It does not claim new benchmark results, customer testimony or first-hand experiments that were not performed.

## Wolven Tech content changes

Two service pages separate direct engineering help from investment/acquisition review. Scope, exclusions, intake information and report structure are visible. An example diligence finding is labelled illustrative. Both pages link from homepage service cards, link to each other, appear in the sitemap and lead to the existing contact page.

Local browser tests exposed a canonical mismatch: metadata resolved to `wolventech.io` while the sitemap used `wolventech.com`. A shared site-origin constant now drives layout metadata, organization identity and sitemap. The existing environment variable used elsewhere is not changed. No production configuration claim follows from the local finding.

Wolven Tech implementation lives in `apps/wolventech` within the portfolio monorepo. Its own implementation note is `docs/reports/2026-10-01-wolventech-search-content.md` there.

## Verification and limits

Focused checks cover rendered article structure, internal routes, sitemap inclusion and removal of unsupported claims. Local production builds and browser checks validate technical delivery; they do not prove indexing, rankings or conversion gains.

Observed results:

- AllSource: 11 tests passed across the content contract, blog metadata, image gate and existing pattern suite. Production build and TypeScript validation passed.
- AllSource: two Playwright tests passed with an owned production server, checking article canonicals, raster social images, comparison-table overflow at desktop/mobile widths and product navigation. Run from `tooling/e2e` with `bunx playwright test --config playwright.seo.config.ts` after building the web app.
- AllSource: both articles, product page and pattern hub returned HTTP 200 locally, each with one H1 and the expected canonical. All four stayed within 360px and 1440px viewports.
- Wolven Tech: production build, separate TypeScript check and focused lint passed. Two browser tests passed, covering both service pages at desktop/mobile widths and navigation through to contact.
- codex-seo: 10 of 10 static HTML checks passed for the CQRS article, event-store product page and each new Wolven Tech service page. These are bounded tag/content checks, not a site-wide SEO score.

Contact navigation is checked without submitting an enquiry. Wolven Tech's local build reports missing Calendar and Resend configuration. This does not establish their production state, and no email-delivery claim is made.

After a separately authorized deployment, inspect each public URL for status, canonical host and visible content. Compare complete 28-day Search Console windows by landing page and relevant query cluster, with low-sample caveats. Measure qualified enquiry completion for Wolven Tech and verified workload/proof completion for AllSource only after checking event definitions and coverage. Page views are not those outcomes.

## Technical references

- [Microsoft: CQRS](https://learn.microsoft.com/en-us/azure/architecture/patterns/cqrs)
- [Microsoft: Event sourcing](https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing)
- [Chris Richardson: Transactional outbox](https://microservices.io/patterns/data/transactional-outbox.html)

These support the architectural distinctions. They do not establish an AllSource-specific performance or feature claim.
