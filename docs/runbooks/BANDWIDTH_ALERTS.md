# Control-plane bandwidth alerts

Status: production evaluator and private email delivery are active on Fly app
`allsource-bandwidth-alerts`. Firing and resolved test emails reached the intended
Gmail inbox on 2026-10-21; sender DKIM, SPF, and DMARC checks passed. The financial
audit remains separate and private.

The rule source is [bandwidth.rules.yml](../../deploy/prometheus/bandwidth.rules.yml).
It watches Fly's edge-output counter independently of application query metering.
Bytes use decimal GB/MB. Edge output is an operational signal, not a complete
invoice reconciliation or a determination that usage is abusive.

| Guard | Threshold | Pending period |
| --- | --- | --- |
| Daily warning | More than 10 GB in rolling 24 hours | 5 minutes |
| Daily critical | More than 50 GB in rolling 24 hours | 5 minutes |
| Sustained critical | More than 1 MB/s, averaged over 5 minutes | 10 minutes |
| Missing measurements | No edge-output samples for 15 minutes | 5 minutes |

These are initial operational thresholds. Review them against measured successful
workload after the read optimizations are deployed. The rules aggregate instances
after applying counter-reset-aware `rate`/`increase`; unrelated apps are excluded.
Alerting does not cap usage or stop requests. Automatic rejection needs an
explicit per-tenant byte budget and an override for approved bulk workloads.

## Production evaluator and delivery

Fly's [metrics documentation](https://fly.io/docs/monitoring/metrics/) requires an
external Grafana alert evaluator or a self-managed Prometheus/Alertmanager setup.
The managed Fly Grafana does not provide that evaluator. The checked-in admin
alert API stores definitions in memory and is not a notification delivery path.

The worker source is [tooling/monitor-stack](../../tooling/monitor-stack/).
It uses Prometheus 2.51.0 and Alertmanager 0.27.0, matching the repository's
Prometheus rule-test image. A Rust supervisor owns both child processes, requests
graceful shutdown, and kills then reaps a child that exceeds its shutdown budget.
Either child exiting restarts the stack through Fly's `always` restart policy.

One shared-CPU 256 MiB machine runs in IAD. A 1 GB encrypted volume retains
Prometheus history and Alertmanager notification state. There are no public IPs
or public services; both HTTP listeners bind to loopback. Fly federation scrapes
only the control-plane edge-output counter every 15 seconds. Rule evaluation is
every minute for bandwidth rules and every 15 seconds for health rules.

The deployed copy of the bandwidth rules is checked against the canonical
source by a Rust test. Source scraping failure becomes critical after two
minutes. Missing counter samples and email-send failures have separate rules.
The local-only Compose monitoring stack is independent of this Fly worker.

Credentials and recipient live in Fly secrets: `FLY_METRICS_TOKEN`,
`RESEND_API_KEY`, `ALERT_FROM`, and `ALERT_TO`. The Fly token is read-only for the
AllSource organization, with a 90-day lifetime; rotate before 2027-01-19. Runtime
credential files have mode 0600 under a mode-0700 directory. The existing Resend
mail configuration supplies the sender and SMTP credential. Port 587 requires
TLS. Values are transferred through private process input, never CLI arguments.

Notifications group by service and alert name. Daily critical inhibits the daily
warning; unresolved alerts repeat every six hours. Recovery emails are enabled.
Email content includes status, service, summary, action, and a working link to
the authenticated Fly metrics dashboard. It excludes request payloads, tenant
identities, credentials, and the financial report. The GitHub disk-alarm public
issue path is not used for these notifications.

Daily volume reflects samples held by this evaluator. First startup needs a full
24 hours to fill that window; rate detection operates while history builds.
History survives restarts. A planned restart was checked through the Prometheus
engine: retained source samples increased from 127 to 147 rather than resetting.
This is a single-worker deployment. A complete host or email-provider outage
can prevent delivery; a failed-email rule cannot independently repair its own
transport. Alerts do not enforce a transfer cap.

### Verified deployment

- Machine: `1850225cd62968`, IAD, restart policy `always`, no public IPs or services.
- Image: `registry.fly.io/allsource-bandwidth-alerts@sha256:98bc9139c32c1f9075f6099d013044ff2dbbc93495cff1e0fdd74dffac2fcea0`.
- Canonical and installed bandwidth rule SHA-256:
  `f256880145f1c52306767d3228783616ed9d2e3810ebda810beffb671fbee205`.
- Final-template firing and resolved test messages were visible in the intended
  Gmail inbox at 16:38 and 16:41 Europe/Lisbon on 2026-10-21. Both use a TEST
  annotation. The HTML dashboard link was checked in the received message.
- Final Alertmanager metrics showed two email notifications and zero email
  failures. The temporary test group was absent; all six production rules
  reported healthy evaluation. The live rate guard was pending, so the test
  recovery does not establish that real bandwidth usage recovered.
- Six Rust tests, Clippy with warnings denied, Prometheus rule tests, and
  offline validation of the deployed image's evaluator/router configs passed.

## Operator checks and delivery verification

Run these from an authenticated Fly CLI session. Commands only reach the private
worker through SSH:

```sh
gtimeout -k 5 30 fly ssh console -a allsource-bandwidth-alerts \
  -C 'curl --fail --max-time 10 http://127.0.0.1:9090/api/v1/targets'
gtimeout -k 5 30 fly ssh console -a allsource-bandwidth-alerts \
  -C 'curl --fail --max-time 10 http://127.0.0.1:9090/api/v1/rules'
```

For an authorized delivery check, run `/bin/monitor-stack probe-start` through
the same SSH command. Wait for the `BandwidthEmailDeliveryProbe` firing email
and verify its TEST annotation and inbox receipt. Run `probe-stop` and verify
the matching resolved email, then `probe-clean`. The probe uses its own rule
file and cannot declare a real bandwidth incident resolved. Confirm the test
group is gone and production rules remain healthy afterward.

Rotate secrets with the `monitor-stack import-secrets` command, supplying an
existing private mail-env path, app name, confirmed recipient, and organization.
It stages a new read-only Fly token plus mail settings without printing values.
Deploy to apply staged secrets, verify fresh scrapes and delivery, then revoke
the replaced read-only token. Never put the source env file into a build context.
The Docker context is restricted to the monitor tool and excludes `.env*`.

## Local rule verification

Use the repository's pinned monitoring image:

```sh
docker run --rm --network none --entrypoint /bin/promtool \
  --mount type=bind,source="$PWD/deploy/prometheus",target=/rules,readonly \
  --workdir /rules prom/prometheus:v2.51.0 \
  test rules bandwidth.rules.test.yml
```

Tests cover quiet healthy traffic, sustained spikes, daily accumulation across
instances, counter resets, unrelated applications, and missing samples. These
prove rule evaluation; they do not prove production notification delivery.
