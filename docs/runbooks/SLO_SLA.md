# AllSource SLO / SLA Runbook

**Status:** stub — targets and alerting wiring are TODO. Owner: Decebal.

This runbook defines the service-level objectives (SLOs) and alerting for AllSource's hosted stack on Fly.io. Pair with `docs/launch/LAUNCH_CHECKLIST.md` — the Phase F items live here, not in the checklist.

## Scope

Applies to the production deployments of:

- `allsource-core` (Fly)
- `allsource-query` (Fly)
- `allsource-control-plane` (Fly) — public API gateway at `https://api.all-source.xyz`
- `allsource-prime` (Fly)
- `allsource-auth` (Fly)
- `allsource-web` (Fly) — marketing site and dashboard at `https://www.all-source.xyz`
- `allsource-admin` (Fly) — admin console at `https://admin.all-source.xyz`

### Custom domains

| Domain | Points to | DNS provider | Record type |
|---|---|---|---|
| `all-source.xyz` / `www.all-source.xyz` | `allsource-web` on Fly | Unstoppable Domains | A `66.241.125.155` + AAAA `2a09:8280:1::180:38b1:0` |
| `admin.all-source.xyz` | `allsource-admin` on Fly | Unstoppable Domains | A `66.241.124.175` + AAAA `2a09:8280:1::180:38b2:0` |
| `api.all-source.xyz` | `allsource-control-plane` on Fly | Unstoppable Domains | A `66.241.125.106` + AAAA `2a09:8280:1::d4:42b8:0` |

Fly manages TLS for all four public hosts via `fly certs`. See `docs/runbooks/FLY_FRONTENDS.md` for frontend deploy, verification, and rollback commands.

## SLO targets — TODO

Fill these in before the first paid Growth customer. The numbers below are placeholders drawn from typical dev-infra SaaS targets; confirm against real production data from Prometheus / Fly metrics before committing to them externally.

| Metric | Target | Measurement window | Data source |
|---|---|---|---|
| Core `/api/v1/events` ingest p99 latency | TBD (placeholder: < 50 ms) | rolling 30 days | Core Prometheus `/metrics` |
| Core `/api/v1/events/query` p99 latency | TBD (placeholder: < 100 ms) | rolling 30 days | Core Prometheus `/metrics` |
| Query Service `/health` uptime | TBD (placeholder: 99.9%) | rolling 30 days | Fly health checks + external pinger |
| Control Plane `/health` uptime | TBD (placeholder: 99.9%) | rolling 30 days | Fly health checks + external pinger |
| 5xx error rate (all backend services) | TBD (placeholder: < 0.1%) | rolling 7 days | Prometheus `http_requests_total{status=~"5.."}` |

**Open questions before publishing:**

1. Do we differentiate write-path (ingest) vs read-path (query) SLOs? Probably yes — the performance characteristics are different.
2. What's the error budget policy? (e.g., freeze non-critical deploys if budget is burned down > 50%.)
3. Do we publish the SLOs on a status page, or keep them internal until we have a Growth/Enterprise customer who asks?
4. Does Enterprise get a contractual SLA (with service credits), or do we offer SLOs without a credit structure initially?

## Alerting — TODO

None of this is wired up yet. Tracking items:

- [ ] Pick an alerting transport: PagerDuty (paid, proper on-call rotation) or Slack webhook (free, no escalation). For a solo operator, Slack is probably enough at launch — revisit when there's a second engineer.
- [ ] Define alert rules in Prometheus (or Fly's built-in health check alerts as a stopgap):
  - Core `/health` failing for > 2 min
  - Query Service `/health` failing for > 2 min
  - Control Plane `/health` failing for > 2 min
  - 5xx rate > 1% over 5 min on any backend service
  - Core ingest p99 > 200 ms sustained for 10 min (placeholder threshold)
- [ ] Decide on an **external** pinger (e.g., Better Stack, UptimeRobot, Checkly) so Fly-internal failures still page. Fly's own health checks can't alert if Fly itself is the problem.
- [ ] Document the on-call response: acknowledge window, escalation path (currently: just Decebal), post-incident note location.

## Incident response — TODO

Flesh out once alerting is in place. Skeleton:

1. Acknowledge the page.
2. Check `fly status -a <app>` and `/health` endpoints directly.
3. Check Core metrics: are events flowing? Is ingest latency degraded?
4. Check recent deploys: `fly releases -a <app>` — most incidents correlate with a deploy.
5. If a rollback is needed, `fly releases rollback <version> -a <app>`.
6. Post-incident: write a short note in `docs/runbooks/incidents/YYYY-MM-DD-<slug>.md` (directory doesn't exist yet; create on first incident).

## Cross-references

- `docs/launch/LAUNCH_CHECKLIST.md` — Phase F references this runbook
- `apps/core/fly.toml` — Core autoscale + health check config
- `apps/query-service/fly.toml` — Query Service health check config
- `apps/control-plane/fly.toml` — Control Plane health check config
