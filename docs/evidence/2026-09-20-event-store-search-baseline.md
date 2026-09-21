# AllSource event-store search and analytics baseline

Captured: 2026-09-20

## Search Console

Property: `https://www.all-source.xyz/`

Google Search Console's selected three-month report contained an effective data
range of 2026-08-26 through 2026-09-18 (the property has no earlier charted
data):

- 8 clicks
- 651 impressions
- 1.2% click-through rate
- average position 15.5

Visible query evidence:

| Query | Clicks | Impressions | Interpretation |
|---|---:|---:|---|
| `all source` | 3 | 67 | Brand demand |
| `allsource` | 1 | 180 | Brand demand |
| `event source` | 0 | 2 | Early generic category discovery |
| `sourcing database` | 0 | 2 | Early adjacent category discovery |

Impressions are discovery evidence, not commercial validation. Current evidence
supports strengthening one canonical event-store definition page. It does not
support generating a broad cluster of near-duplicate category pages.

Top landing pages:

| Page | Clicks | Impressions |
|---|---:|---:|
| `/` | 5 | 327 |
| `/event-sourcing-for-ai-agents` | 0 | 128 |
| `/pricing` | 1 | 40 |
| `/what-is-allsource` | 0 | 38 |
| `/blog` | 1 | 32 |
| `/platform/event-sourcing` | 0 | 18 |
| `/docs` | 0 | 12 |
| `/blog/reconstructing-agent-memory-in-rust` | 1 | 7 |

Device totals:

| Device | Clicks | Impressions |
|---|---:|---:|
| Mobile | 6 | 157 |
| Desktop | 2 | 492 |
| Tablet | 0 | 2 |

Country evidence is highly fragmented. United States generated 420 impressions
and 1 click. United Kingdom generated 14 impressions and 1 click. India and
Brazil generated 24 and 19 impressions respectively with no clicks. Remaining
visible countries had five or fewer impressions each. This is not enough volume
to create country-specific pages or infer country-level demand.

## PostHog

Production contract:

- PostHog Cloud EU
- `bet=allsource`
- `traffic_role=production`
- `analytics_test=false`
- canonical hosts only: `all-source.xyz`, `www.all-source.xyz`
- cookieless; no autocapture, replay, or person profiles
- no event payloads, entity IDs, credentials, email, names, or free text

Production verification after Fly deployment `6185001b`:

- PostHog client initialized on `https://www.all-source.xyz/`.
- EU project configuration returned HTTP 200.
- event ingestion returned HTTP 200.
- identity/configuration endpoint returned HTTP 200.
- feature-flag endpoint returned HTTP 200.

Production verification after event-store discovery deployment `abbdeb54`:

- `https://www.all-source.xyz/what-is-an-event-store` returned HTTP 200 from Fly.
- Canonical resolved to the clean production URL without the verification query.
- Title, description, one H1, breadcrumbs, `TechArticle`, and organization/site
  structured data rendered in the live document.
- `robots.txt` allowed the page and declared the production sitemap.
- `sitemap.xml` contained the definition page, API docs, hub, and ten pattern pages.
- Homepage hero, docs hub, and footer rendered links to the definition page.
- 390 px viewport rendered at 390 px document width with no horizontal overflow.
- Activating the `Read API docs` CTA loaded `/docs/api` with HTTP 200 and produced
  a PostHog EU ingestion response with HTTP 200.
- Targeted tests passed (9 tests), and the production Next.js build generated 111
  pages after type checking.

At the initial capture, dashboard aggregates were unknown because the authenticated
Chrome account could see project `33987`, not the production shared project `244095`.
This access blocker was resolved on 21 September; see the follow-up below.

## Dashboard follow-up — 21 September 2026

The authenticated account now exposes https://eu.posthog.com/project/244095/activity/explore.
Last-30-day Activity with `bet=allsource`, `traffic_role=production`,
`analytics_test=false`, and internal/test-user filtering enabled returned all
36 matching records: 13 pageviews, 14 web-vitals events, eight pageleaves and one
`marketing_cta_clicked`. The CTA originates from `/what-is-an-event-store`, with
a subsequent `/docs/api` pageview. No signup events appeared in this bounded result.

These are **production-labelled records, not verified customer conversions**.
Code inspection identified that the initial integration set `analytics_test`
solely from hostname, ignoring explicit QA query markers. The documented 20
September docs CTA check is therefore included in the production-labelled data.
Do not claim the one CTA as organic acquisition, infer a conversion rate, or
treat absent PostHog signups as zero authoritative signups.

Correction tracked in `t-539203`: explicit `analytics_test=1` or
`analytics_test=true` classifies the tab as test traffic, persisting only a
boolean QA flag in sessionStorage across query-free navigation. A same-document
in-memory fallback handles unavailable storage; a hard reload with storage
blocked requires the explicit marker again. No user identifier or arbitrary
query value is stored or transmitted. Query strings remain stripped from URLs.

Signup/activation reconciliation against authoritative AllSource data is still
outstanding. `t-7ba26f` must remain open until that acceptance criterion is met.

## Measurement path

`/what-is-an-event-store` emits `$pageview`. Its fixed CTA allowlist emits
`marketing_cta_clicked` for `api_docs`, `live_demo`, or `signup`. Signup emits
`signup_started` and `signup_accepted`. AllSource product events remain authority
for activation and durable product outcomes; PostHog is directional acquisition
and UX evidence.
