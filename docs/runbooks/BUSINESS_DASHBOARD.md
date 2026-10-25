# Business dashboards

Three dashboards in Fly's managed Grafana. Each section is a question, and the first panel in it is the answer.

| Dashboard | Questions | Link |
|---|---|---|
| Business Questions | Are customers getting their data right now? Which tenants can't read their data back? Which tenants cost more than they pay? Which tenants should run their own deployment? What does traffic cost us? Can Core hold every tenant it has to serve? | https://fly-metrics.net/d/allsource-business?orgId=1483699 |
| Availability & Incidents | Are callers getting answers? Is Core refusing work, and for whom? Did a restart or deploy come first? Is Core busy, or idle while callers fail? Which tenants were hit? | https://fly-metrics.net/d/allsource-availability?orgId=1483699 |
| Engineering Detail | Raw traffic by app and region, tenant traffic, query shapes and gzip, admission refusals, capacity, Core workload | https://fly-metrics.net/d/allsource-engineering?orgId=1483699 |

The sources of truth are the JSON files in [`deploy/grafana/dashboards/`](../../deploy/grafana/dashboards/): `business-overview.json`, `availability.json` and `engineering-detail.json`. Change the files, not the dashboards in the browser. An edit made only in Grafana is lost on the next import, and nobody reviews it.

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

`allsource-bandwidth-alerts` (tooling/monitor-stack) emails on:

- `ControlPlaneServingFailures`: 5xx at least half of control-plane responses for 10 minutes ([`rules/availability.yml`](../../tooling/monitor-stack/rules/availability.yml)). Replayed against 2026-10-24, it fires by 22:25Z, 34 minutes into an outage nobody saw for 12 hours.
- `ControlPlaneSuccessCollapsed`: successful responses under 5% of the same time yesterday for 15 minutes. A 20% threshold would have fired on the throttling the evening before, so 5% is the line.
- `CoreMetricsMissing`, `ControlPlaneMetricsMissing`, `QueryServiceMetricsMissing`: an app's metrics stopped arriving for 15 minutes ([`rules/scrape-health.yml`](../../tooling/monitor-stack/rules/scrape-health.yml)).

All five are tested with `promtool` in the Bandwidth Alerts workflow (`tooling/monitor-stack/tests/`).
