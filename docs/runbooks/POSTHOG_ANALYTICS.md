# AllSource PostHog analytics

## Production authority

AllSource web acquisition and product-UX analytics use PostHog Cloud EU. Fly
builds compile the public write-only project token and EU ingestion host from
`apps/web/fly.toml`.

AllSource Core remains authority for durable product outcomes, customer event
history, and AI-referral provenance. PostHog is not a second product database.

## Privacy contract

- `bet=allsource`
- `tracking_schema=1`
- `traffic_role=production` only on `all-source.xyz` and `www.all-source.xyz`
- `analytics_test=false` only on canonical production hosts
- cookieless mode always
- autocapture disabled
- session replay disabled
- person profiles disabled
- URLs reduced to origin plus pathname
- no event payloads, entity IDs, API keys, email addresses, names, form bodies,
  or free text

## Events

- `$pageview`
- `$pageleave`
- `$web_vitals`
- `signup_started` with fixed `method`
- `signup_accepted` with fixed `method` and boolean `new_user`
- `dashboard_event_created` without event content
- `onboarding_sdk_selected`
- `onboarding_event_created`
- `onboarding_query_completed` with aggregate result count
- `onboarding_completed`

Every production insight must filter `bet=allsource`,
`traffic_role=production`, and `analytics_test=false`. Controlled verification
must use a non-canonical hostname or explicit test traffic and never count as
demand.

## Verification

1. Deploy `allsource-web` from repository root with
   `fly deploy . --config apps/web/fly.toml --remote-only --ha=false`.
2. Open `https://www.all-source.xyz/` and one marketing route.
3. Confirm browser requests reach `https://eu.i.posthog.com` without CSP errors.
4. In PostHog, filter project events by `bet=allsource` and
   `traffic_role=production`.
5. Confirm exactly one `$pageview` per navigation and no query-string or user
   payload properties.
6. Keep dashboard URLs or exports in dated evidence; public project token does
   not grant read access.
