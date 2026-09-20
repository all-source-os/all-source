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

Dashboard aggregates remain unknown because the authenticated Chrome account can
see project `33987`, while production uses shared project `244095`. Successful
browser ingestion proves delivery, not a dashboard count or conversion rate.

## Measurement path

`/what-is-an-event-store` emits `$pageview`. Its fixed CTA allowlist emits
`marketing_cta_clicked` for `api_docs`, `live_demo`, or `signup`. Signup emits
`signup_started` and `signup_accepted`. AllSource product events remain authority
for activation and durable product outcomes; PostHog is directional acquisition
and UX evidence.
