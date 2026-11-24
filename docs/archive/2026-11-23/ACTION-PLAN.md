# AllSource SEO and performance action plan

Updated: 2026-09-02

## Completed now

- [x] Restore homepage ISR and public cache headers.
- [x] Isolate cheap health endpoint from homepage rendering.
- [x] Bound live pricing catalog fetch.
- [x] Remove client-delayed hero proof from LCP path.
- [x] Repair and cache dynamic social image.
- [x] Stop preloading below-fold homepage article images.
- [x] Fix mobile announcement density.
- [x] Reach Lighthouse Accessibility, Best Practices, and SEO 100.

## P1 — next performance pass

- [ ] Add metadata-only blog index; compile MDX only on article routes.
- [ ] Measure GA conversion value, then defer or remove 171 KB third-party script.
- [ ] Decide whether second Fly machine is worth added monthly cost.
- [ ] Load-test and lower request concurrency limits to protect single shared CPU.

## P2 — search quality

- [ ] Repair title and description length outliers on secondary pages.
- [ ] Add contextual links from technical articles to agent-memory commercial hub.
- [ ] Add LinkedIn and Product Hunt to Organization `sameAs`.
- [ ] Add visible source links and verification dates to benchmark claims.
- [ ] Remove HTTP apex extra redirect if DNS/proxy path permits one-hop canonicalization.

## Acceptance checks for future work

- Mobile Lighthouse median Performance stays at least 90.
- Accessibility, Best Practices, SEO remain 100.
- Homepage cache remains HIT with no server cookie dependency.
- 20 consecutive health requests pass.
- No increase in third-party or first-party transfer without measured conversion value.
