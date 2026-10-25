# Business dashboards

Five dashboards in Fly's managed Grafana. Each section is a question, and the first panel in it is the answer.

| Dashboard | Questions | Link |
|---|---|---|
| Business Questions | Are customers getting their data right now? Which tenants can't read their data back? Which tenants cost more than they pay? Which tenants should run their own deployment? What does traffic cost us? Can Core hold every tenant it has to serve? | https://fly-metrics.net/d/allsource-business?orgId=1483699 |
| Availability & Incidents | Are callers getting answers? Is Core refusing work, and for whom? Did a restart or deploy come first? Is Core busy, or idle while callers fail? Which tenants were hit? | https://fly-metrics.net/d/allsource-availability?orgId=1483699 |
| Engineering Detail | Raw traffic by app and region, tenant traffic, query shapes and gzip, admission refusals, capacity, Core workload | https://fly-metrics.net/d/allsource-engineering?orgId=1483699 |
| Egress Attribution | How many bytes leave the control plane's edge, and what share a route accounts for? Which route and tenant? Which event read (namespace and shape)? Who is refused? The bandwidth alert emails link here | https://fly-metrics.net/d/allsource-egress?orgId=1483699 |
| Traffic & Outage Report | The [production traffic report](../reports/2026-10-24-production-traffic.html) as a live dashboard: what happened, did callers get answers, was Core refusing work and for whom, the timeline and cause, who uses the bandwidth, throttling and capacity. Opens on the 2026-10-24/25 outage window | https://fly-metrics.net/d/allsource-traffic-report?orgId=1483699 |

The sources of truth are the JSON files in [`deploy/grafana/dashboards/`](../../deploy/grafana/dashboards/): `business-overview.json`, `availability.json`, `engineering-detail.json`, `egress-attribution.json` and `traffic-outage-report.json`. Change the files, not the dashboards in the browser. An edit made only in Grafana is lost on the next import, and nobody reviews it.

## Event markers

The red markers and shaded bands on the Traffic & Outage Report are Grafana annotations tagged `allsource-event`, listed in [`deploy/grafana/annotations/allsource-events.json`](../../deploy/grafana/annotations/allsource-events.json): deploys, the resize, the outage window, the tenant refusals and when the new alert would have fired. Grafana stores annotations in its own database, not in the dashboard JSON, so this file is their source. To add one, append it to the file, land it through a PR, then create it in Grafana with `POST /api/annotations` (`time` and optional `timeEnd` in epoch milliseconds, `text`, `tags`), skipping any entry whose time and text already exist. Core and control-plane restarts need no entry: the dashboard derives them from `resets(fly_instance_cpu{mode="idle"})`.

## Which caller sends the bytes

When a `ControlPlaneDailyBandwidth*` or `ControlPlaneSustainedBandwidthCritical` alert fires, open Egress Attribution and read it top down:

1. **Share attributed to a route.** Expect close to 100%: event-query bodies alone were 97.9% of edge output over 27.6 hours on 2026-10-25/26 (19.23 of 19.64 GB, release v121, from the public shape counter since process start against `increase(fly_edge_data_out[99310s])`). Under 90% means traffic no route meters, or a scrape gap. *not scraped* means the running control-plane release has no `[metrics]` block for port 9091, so sections 2 and 3 are empty. Use the logs panel at the end of section 3 until it is deployed.
2. **Bytes by route, then by tenant and route.** `/api/v1/events/query` is usually most of it.
3. **Event-query bytes by tenant, namespace and read shape.** The namespace is the first segment of the `event_type` or `event_type_prefix` (before `.`) or of the `entity_id` (before `:`). A row that is time-bounded, payload-filtered and has a large *Bytes per query* is usually a client replaying a whole stream on every call. Find that client by its namespace and shape, then cut the read there (cache it, page it, or poll with `If-None-Match`). The *Polls answered 304* stat shows how much conditional polling already saves.
4. **Sampled large responses (logs).** One sample at most every 10 seconds, responses of 16 KiB or more, tenant as a SHA-256 fingerprint. Compare rows with each other; the counts are samples, not requests.

The control plane's public `/metrics` also carries `control_plane_event_query_body_bytes_by_shape_total` (bytes by selector and shape, no tenant) since process start. It is not scraped; `curl https://allsource-control-plane.fly.dev/metrics` reads it when the private port is not scraped either.

Worked example, 2026-10-25 (control plane v121, `300f852`, no `[metrics]` block). Edge output ran at 0.85–0.92 GB/h, about 21 GB/day. Over the ~23 h since the release, the public shape counter attributed 13.9 GB to `/api/v1/events/query`, 7.29 GB of it to time-bounded, payload-filtered `event_type_prefix` reads (24,115 queries, 302 KB each). The logs panel split that shape between the `skill` namespace (87 samples, 532 KB average) and `team_queue` (133 samples, 118 KB average). Both are the Longhand control-plane tenant. The `skill` read is Longhand's workspace skill-catalog replay (`event_type_prefix=skill.`, `payload_filter=owner_workspace_id`, `until=now`), which re-reads every published manifest on each call.

## Where the numbers come from

| Data | Series | Source |
|---|---|---|
| Responses, egress, machines | `fly_edge_*`, `fly_instance_*` | Fly, for every app |
| Per-tenant requests, bytes, refusals | `control_plane_tenant_*`, `admission_*` | control plane `:9091/metrics` |
| Per-tenant plan price | `control_plane_tenant_plan_monthly_usd{tenant,tier,status}` | control plane, refreshed every 5 minutes from the billing price list |
| Core HTTP traffic, uncompressed bytes | `allsource_http_*` | Core `:3900/metrics` |
| Core refusing to load a tenant; Core 503s | log lines `refusing to hydrate tenant`, `503 Service Unavailable` | Core logs, through the *Application Logs (VictoriaLogs)* datasource |

Fly scrapes a `[metrics]` endpoint every 15 seconds and keeps metrics for about 15 days ([docs.fly.io/monitoring/metrics](https://docs.fly.io/monitoring/metrics)). A panel reads *No data* until the app behind it is deployed with its `[metrics]` block. *No data* means not measured, never zero.

## The rules the answers use

- **Cost more than it pays:** plan list price minus the tenant's response bytes over the window, scaled to 30 days and priced at $0.02/GB. Only egress is metered per tenant; machines and storage are shared, so a positive margin is an upper bound. Enterprise prices are negotiated and read $0.
- **Run their own deployment:** one point for each of: projected egress cost at least the *Dedicated stack $/month* filter; share of all tenant response bytes at least the *Fleet share* filter; refused requests over requests at least the *Refused share* filter. 0 stay on shared, 1 watch, 2 or 3 recommend a dedicated deployment or self-hosted Community Edition. *Dedicated stack $/month* defaults to 100 as an assumption; set it from the Fly invoice.
- **Egress dollars:** Fly's published rates by edge region ([docs.fly.io/about/pricing](https://docs.fly.io/about/pricing), read 2026-10-25): $0.02/GB North America and Europe, $0.04/GB Asia Pacific, Oceania and South America, $0.12/GB Africa and India. If Fly changes them, update the multipliers in `business-overview.json`.

## Updating a dashboard

1. Edit the JSON and run every changed query in Grafana Explore against **Prometheus on Fly** or **Application Logs (VictoriaLogs)**.
2. Land it through a PR.
3. Import the merged file: **Dashboards → New → Import**, paste it, confirm the overwrite of its uid. Grafana keeps earlier versions under **Settings → Versions**.

## Alerts

`allsource-bandwidth-alerts` (tooling/monitor-stack) emails on the bandwidth rules in [`rules/bandwidth.rules.yml`](../../tooling/monitor-stack/rules/bandwidth.rules.yml) (10 GB and 50 GB of control-plane edge output in 24 hours, 1 MB/s for 10 minutes), whose emails link to Egress Attribution through each rule's `dashboard` annotation, and on:

- `ControlPlaneServingFailures`: 5xx at least half of control-plane responses for 10 minutes ([`rules/availability.yml`](../../tooling/monitor-stack/rules/availability.yml)). Replayed against 2026-10-24, it fires by 22:25Z, 34 minutes into an outage nobody saw for 12 hours.
- `ControlPlaneSuccessCollapsed`: successful responses under 5% of the same time yesterday for 15 minutes. A 20% threshold would have fired on the throttling the evening before, so 5% is the line.
- `CoreMetricsMissing`, `ControlPlaneMetricsMissing`, `QueryServiceMetricsMissing`: an app's metrics stopped arriving for 15 minutes ([`rules/scrape-health.yml`](../../tooling/monitor-stack/rules/scrape-health.yml)).

All five are tested with `promtool` in the Bandwidth Alerts workflow (`tooling/monitor-stack/tests/`).
