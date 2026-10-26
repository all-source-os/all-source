---
name: allsource-incident-status
description: Produce a measured-only incident status report for AllSource and the products that depend on it - who is blocked, why, and the current state - with every value timestamped and every unmeasured point listed as unknown. Use when something is down or flapping, when a user asks "who is blocked", "why is X failing", "what's the current status", "give me the data", or after a deploy when a dependent product reports errors. Read-only; never deploys, rotates keys, or asks the user to change credentials.
---

# Incident status: data, not a story

The report answers three questions from measurements: **who is blocked, why, and what the state is now**. It never offers a cause it did not observe, an identifier it did not read, or a fix that moves a platform regression onto the user.

On 2026-10-26 a dependent product's sign-in returned 503. Over one session the reporting was replaced three times by a confident narrative: a key-rotation instruction, a login address reconstructed from a tenant slug, and a cause chosen from four overlapping deploys. Each was wrong or unproven, and the user had to stop the work to ask for the data. This skill is the shape that was asked for instead.

## Rules

1. **Every value carries its UTC timestamp and its source command.** Run `date -u +%Y-%m-%dT%H:%M:%SZ` first and quote it as the report time.
2. **Unmeasured means unknown.** Write it under `Unknowns`, never as a likely cause. When several deploys overlap the failure window, list them all; do not attribute the outcome to one.
3. **Never present an inferred identifier as fact.** A login, email, tenant owner or account reconstructed from a naming pattern is either labelled `inferred from <source>` or left out.
4. **Platform regression, platform fix.** If an AllSource change broke existing callers, the report says so and does not tell the caller to rotate keys, reissue credentials, or change their configuration.
5. **Read-only.** No deploy, rollback, secret change, key creation, or workflow dispatch that changes state. Propose them separately, after the report, and only if asked.
6. **No prose between tables.** One line under a table is allowed when a number needs its unit or caveat. No conclusions section.
7. **Each probe is bounded.** Wrap every command in `gtimeout -k 5 40` (or `timeout`). A probe that times out is reported as `timeout`, not skipped.

## Probes

Run independent probes in parallel.

### A. Report time

```bash
date -u +%Y-%m-%dT%H:%M:%SZ
```

### B. The failing flow, from the outside

Hit the exact endpoint the user or dependent product reported, unauthenticated where the endpoint is public. Record status and latency.

```bash
curl -sS -m 20 -o /dev/null -w "%{http_code} %{time_total}s\n" "<endpoint the user named>"
```

Probe a known-good sibling too (the same endpoint without the failing parameter, or `/health`) so a 503 can be placed: whole service down versus one path failing.

### C. Deploys in the window

```bash
fly releases -a allsource-core --image
fly releases -a allsource-query --image
fly releases -a allsource-control-plane --image
```

List every release whose time falls in the incident window, with version, status (`complete` / `interrupted` / `failed`), image, and user. Add merged PRs in the same window:

```bash
gh pr list -R all-source-os/all-source --state merged --search "merged:>=<window start>" --json number,title,mergedAt
```

An image deployed by digest from a branch can be live before its PR merges. Compare the release time with `mergedAt`.

### D. Machine state and health

```bash
fly machines list -a allsource-core --json
```

Read `state`, the `checks[].status` and `checks[].output`, and `updated_at`. Core's health output reports `system_streams.total_events`.

### E. Core boot, when Core restarted in the window

```bash
fly logs -a allsource-core --no-tail
```

Record the timestamps of `Stage 1`, `Stage 2`, `Stage 4 ... ready to accept traffic` and `listening on [::]:3900`. Core accepts no connections until Stage 4. Between them, the gateway logs `connection refused` on `allsource-core.internal:3900` and every login and query fails. Report the outage window as shutdown time to listening time.

### F. Gateway refusals

```bash
fly logs -a allsource-control-plane --no-tail
```

Count, per minute window: `egress refused status=429 reason=busy`, `status=503 reason=ledger_unavailable`, `status=402`, and `connection refused` to Core. Note restarts (`Shutting down gracefully`, `listening on port 3901`).

The gateway does **not** log authentication failures. An absence of 401 lines says nothing about whether callers are being rejected. A 401 is measured only at the caller.

`event_query_large_response` lines carry `tenant_fingerprint`. Use it to show that a tenant's reads are reaching Core again; do not reverse it into a tenant name.

### G. Dependent product health

If the affected product exposes a health or ops endpoint, read it. Lifetime counters (failures since process start) are not a rate. Take two samples and compare a monotonic field such as the request count first: if it went down, the process restarted between samples and the counters cannot be subtracted.

### H. Dependent product CI and ops runs

```bash
gh run list -R <owner>/<repo> --branch main --limit 6 --json workflowName,conclusion,status,createdAt,headSha
```

Quote the exact error line from a failed run's log rather than its conclusion.

## Report format

```
Data as of <A>.

1. <Failing flow>
| Time (UTC) | Target | HTTP | Latency |

2. AllSource deploys in window
| App | Version | Time (UTC) | Status | Image |
Merged PRs: #<n> at <time>.

3. Core
Down <start>-<end> (<source line>). Boot replayed <n> system events; Stage 2 <duration>.
Health <passing|failing> since <time>.

4. Gateway refusals
| Window (UTC) | 429 busy | 503 ledger_unavailable | 402 | Core refused |

5. <Dependent product> health
<fields read, with timestamp; restart between samples yes/no>

6. <Dependent product> CI
<run, sha, conclusion, quoted error line>

Unknowns
- <every point the probes did not measure, including which change caused or ended the failure when deploys overlap>
```

Who is blocked is answered by rows, not prose: a flow with a non-2xx row at the latest time is blocked; one whose latest row is 2xx is not, and the report states which other flows were not tested.

## Host capability

Codex: invoke as $allsource-incident-status or request its name naturally. Use native repository tools and its current sandbox. A nested host command requires the same applicable authority as a direct call.
