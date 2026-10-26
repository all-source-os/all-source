# Consumer access gate

A deploy of `allsource-control-plane` or `allsource-query` must never lock out a
credential that customers already hold. Both apps deploy `bluegreen`. The new
machine gets traffic only after its `consumer_access` check passes, and that
check sends one canary credential of every kind customers hold through the
machine's own listener: auth middleware, key lifecycle, egress guard, Core read.
If any kind is rejected, Fly keeps the old machine serving and the deploy fails.

## Why

On 2026-10-26 #363 was deployed to both gateways at 19:30Z and again with #364 at
22:43Z. Each time, long-lived API keys issued before #363 stopped authenticating.
Longhand's control plane holds one of those keys, so Longhand sign-in returned
503 and its reads stopped reaching the gateway: one read every ~10 s from that
tenant before 22:44:30Z, none after. `/livez` stayed green the whole time,
because it only proves the process answers HTTP.

## What the check reads

| App | Endpoint | Probe request per canary |
|---|---|---|
| allsource-control-plane | `GET /readyz/consumers` | `GET /api/v1/events/query?entity_id=consumer-access-canary&limit=1` with `Authorization: Bearer <canary>` against `127.0.0.1:$PORT` |
| allsource-query | `GET /api/health/consumers` | the same request against `127.0.0.1:$PORT` |

- 200 when every canary answers 200. 503 otherwise, with each kind's status. The
  response never contains a token.
- With no canaries configured the endpoint answers 503, so the deploy fails.
  An unconfigured gate is not a passed gate.
- One probe round per 30 s at most; the endpoint is public.
- The check is a top-level Fly `[checks]` entry. It gates deploys and does not
  remove a running machine from routing, so a Core outage after deploy does not
  take the gateway offline.

## The canary set

`CONSUMER_CANARY_CREDENTIALS` is a Fly secret on both apps: a JSON array of
`{"kind": "<label>", "token": "<credential>"}`.

Rules:

1. **One canary per credential kind that customers hold today.** At minimum:
   - an onboarding API key issued **before 2026-10-26**, from the old onboarding
     flow (`SignAPIKey`);
   - a managed dashboard key with no expiry;
   - a managed dashboard key with an expiry;
   - a Core-native `ask_` key, if any customer holds one.
2. **A canary is never re-minted to make the gate pass.** A key minted by the new
   code path proves the new path works, which is not the question. The question
   is whether keys minted by the old paths still work. When issuance changes, add
   a canary for the new kind and keep the old ones.
3. **Retiring a kind is a decision, not a side effect.** Removing a kind from the
   set means every customer holding that kind has been migrated. Record who
   migrated them, and when, in the PR that removes the canary.
4. Canaries belong to an internal tenant with read scope only. Revoke and replace
   one only if it leaks. The replacement must come from the same issuance path,
   which may mean keeping an old build around to mint it.

Set the secret without restarting the running machines:

```bash
fly secrets set --stage -a allsource-control-plane CONSUMER_CANARY_CREDENTIALS='[...]'
fly secrets set --stage -a allsource-query CONSUMER_CANARY_CREDENTIALS='[...]'
```

Never print a token, pass it in a workflow input, or commit it.

## Deploying

Deploy as before (`docs/runbooks/FLY_IMAGE_PROMOTION.md`). The strategy comes
from the app's `fly.toml`, so `fly deploy --image ...` is gated without extra
flags. The new machine passes or the deploy fails. Read the check output:

```bash
fly checks list -a allsource-control-plane
fly checks list -a allsource-query
```

A failing `consumer_access` check during deploy means the build rejects a
credential customers hold. Do not override it with `--strategy immediate` or
`rolling`. Fix the build or, if the kind really is being retired, follow rule 3.

## Limits

- The gate covers the two gateways. Core is deployed with a volume and cannot
  run bluegreen; its changes reach customers through these gateways, so a Core
  change that breaks a credential still fails the next gateway deploy, but not
  the Core deploy itself.
- The probe reads one tenant per credential kind. It does not prove every tenant
  can read, only that each kind of credential is still accepted.
