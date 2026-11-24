# AllSource production SEO and performance audit

Audit date: 2026-09-02  
Canonical origin: `https://www.all-source.xyz/`  
Method: codex-seo specialists plus iterative production autoresearch

## Outcome

Production outage fixed. Root cause was application saturation, not DNS or TLS: public homepage lost ISR through server cookie reads, root health checks repeatedly invoked expensive rendering, and catalog requests had no timeout. One shared Fly CPU accumulated overlapping work until both homepage and health route stalled.

Final mobile Lighthouse: **Performance 90–92, Accessibility 100, Best Practices 100, SEO 100**. Three-run median Performance is 91. CLS is 0; median LCP is 2.93 s. No CrUX field data is available, so lab TBT is not reported as INP.

## Changes shipped

- Restored static/ISR homepage and marketing layout.
- Moved launch-banner dismissal to browser storage.
- Added 2 s catalog timeout.
- Changed container health check from `/` to `/api/healthz`.
- Disabled Vercel Analytics outside Vercel.
- Server-rendered hero event history; removed delayed client animation from LCP content.
- Repaired `/og` 502 and added cache headers.
- Removed priority from below-fold blog images and added responsive image sizes.
- Reduced mobile announcement height.
- Fixed all Lighthouse color-contrast failures.

## Category scores

| Category | Score | Evidence |
|---|---:|---|
| Technical | 92 | DNS, TLS, HSTS, canonical, robots, sitemap, cache, health all pass |
| Content | 82 | Specific, evidence-led copy; strong intent coverage; limited public trust proof |
| On-page | 88 | Clear H1 and intent; metadata outliers remain on secondary pages |
| Schema | 84 | Organization, Person, WebSite, SoftwareApplication, FAQPage present; entity enrichment remains |
| Performance | 91 | Three-run mobile median; LCP 2.93 s; CLS 0 |
| GEO | 88 | Strong `llms.txt`, source-oriented content, AI crawlers allowed |
| Images | 98 | Valid assets and dimensions; homepage priority waste removed |
| Weighted overall | **89** | Current production evidence |

## Technical findings

- Homepage and health route now return 200 consistently. Homepage is ISR with one-hour revalidation.
- Warm homepage TTFB generally 286–463 ms from Lisbon; Fly origin is in `iad`, so network distance remains material.
- `/og` now returns cacheable 1200×630 PNG instead of 502.
- `robots.txt`, `sitemap.xml`, and `llms.txt` return 200.
- Security headers pass. CSP still includes `unsafe-inline`; track separately from load work.
- HTTP apex has two redirects. HTTPS apex correctly redirects once to canonical `www`.

## Content, GEO, and schema findings

- Homepage avoids generic AI copy and explains durable history, provenance, and reconstruction clearly.
- Content audit found good passage-level citability and low AI-slop risk.
- Add visible benchmark verification dates and primary-source links where performance claims appear.
- Add LinkedIn and Product Hunt to Organization `sameAs`.
- Enrich SoftwareApplication with version, release notes, license, download, and help URLs where truthful.
- Avoid copying static prices into `llms.txt`; live catalog is source of truth.

## Remaining performance work

1. Split blog metadata listing from MDX compilation. Current ISR rebuild compiles 41 article bodies to render three cards.
2. Evaluate deferring Google Analytics until consent plus idle/interaction. It contributes about 171 KB transfer and most unused JavaScript.
3. Add second Fly machine if launch traffic and budget justify high availability.
4. Reduce Fly concurrency limits from 200/250 to a tested value near 25/50 if overload protection is preferred over queued saturation.

## Verification

- TypeScript: pass.
- Vitest: 25 files, 110 tests passed.
- Next production build: pass; homepage statically generated with one-hour revalidation.
- Fly deployment: healthy.
- Production requests: homepage 10/10, health 20/20.
