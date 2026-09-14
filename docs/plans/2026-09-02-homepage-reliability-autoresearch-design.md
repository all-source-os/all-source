# Homepage reliability autoresearch design

Date: 2026-09-02
Target: `https://www.all-source.xyz/`

## Problem

Production returned zero-byte responses and timed out for both `/` and `/api/healthz`. Fly machine had one shared CPU and 512 MB RAM. Public homepage was rendered dynamically because server components read banner cookies, while health checks requested expensive `/`. Stalled upstream catalog requests and failing dynamic social images added load.

## Frozen rubric

1. Availability: 20/20 health requests return 200; Fly health check passes.
2. Delivery: 10/10 homepage requests return 200; warm TTFB stays below 500 ms except isolated network variance; page reports `x-nextjs-cache: HIT`.
3. Rendering: homepage builds as static/ISR; no server cookie dependency.
4. Lab UX: mobile Lighthouse median Performance at least 90, Accessibility 100, Best Practices 100, SEO 100, CLS 0.
5. Social discovery: `/og` returns cacheable 1200×630 PNG with HTTP 200.
6. Crawl surface: `robots.txt`, `sitemap.xml`, and `llms.txt` return HTTP 200.

## Method

- Measure DNS/TLS, Fly health, route timing, process pressure, route cache headers, and Lighthouse.
- Change one bottleneck class per experiment.
- Keep only changes that improve rubric without breaking tests or static generation.
- Validate production after each Fly deployment.

## Stop condition

Stop when all rubric checks pass. Defer cost-changing capacity work and non-critical bundle reduction to separate tasks.
