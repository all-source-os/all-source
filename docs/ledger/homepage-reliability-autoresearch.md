# Homepage reliability autoresearch ledger

Date: 2026-09-02

| Run | Change | Evidence | Decision |
|---|---|---|---|
| 0 | Baseline | `/` and `/api/healthz` produced zero bytes or timed out beyond 15–60 s. Fly check critical. One shared CPU saturated; Node RSS about 237 MB. | Reject baseline |
| 1 | Restore ISR and cheap health checks | Removed server `cookies()` reads, added 2 s catalog timeout, moved Docker health check from `/` to `/api/healthz`, disabled Vercel Analytics outside Vercel. Production returned 200 with `x-nextjs-cache: HIT`; warm TTFB 231–306 ms in early verification. | Keep |
| 2 | Server-render homepage proof | Replaced timer-driven client hero demo with static server-rendered event history and recalled answer. Removed motion/runtime delay from LCP content. Post-recovery mobile Lighthouse median Performance rose from 76 to 91. | Keep |
| 3 | Repair social image | Removed unsupported `aspectRatio: auto`, supplied explicit image dimensions, added public cache headers. `/og` changed from 502 to 200 `image/png`, 1200×630. | Keep |
| 4 | Reduce launch-page waste | Removed priority from three blog images about 10,000 px below fold, added responsive `sizes`, shortened mobile Product Hunt banner. | Keep |
| 5 | Fix contrast | Enforced accessible colors for pricing CTA and enterprise button. Lighthouse Accessibility changed 96 to 100. | Keep |

## Final production evidence

- Homepage: 10/10 HTTP 200; nine warm TTFB samples 286–463 ms, one post-deploy cold sample 720 ms.
- Health: 20/20 HTTP 200; TTFB 230–370 ms; Fly check passing.
- Cache: `x-nextjs-cache: HIT`, `x-nextjs-prerender: 1`, `s-maxage=3600`.
- Mobile Lighthouse, three-run median before final contrast-only patch: Performance 91, FCP 1.96 s, LCP 2.93 s, TBT 89 ms, CLS 0, Best Practices 100, SEO 100.
- Final contrast verification: Performance 90, Accessibility 100, Best Practices 100, SEO 100, LCP 2.97 s, CLS 0.
- Social image: HTTP 200, PNG, 1200×630, cacheable for browsers and shared caches.
- Crawl files: `robots.txt`, `sitemap.xml`, `llms.txt` all HTTP 200.

## Remaining observations

- Google Analytics remains largest third-party payload at about 171 KB transfer.
- Homepage ISR rebuild still compiles all 41 MDX article bodies to select three cards.
- One Fly machine remains a single-instance availability risk. A second machine changes operating cost and needs owner approval.
- HTTP apex performs two redirects; HTTPS apex canonical redirect is correct.
