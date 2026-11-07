# @allsourcedev/client

TypeScript/JavaScript client for the [AllSource](https://all-source.xyz) event store API.

Talks to the AllSource gateway (`https://api.all-source.xyz`), not Core directly.
Ships compiled ESM + CJS with `.d.ts` types. Runs on Node 18+ and Bun. Zero
runtime dependencies (uses the global `fetch`, and WebCrypto for client
assertions).

## Installation

```bash
npm install @allsourcedev/client
# or
bun add @allsourcedev/client
# or
pnpm add @allsourcedev/client
```

> **Why not a git install?** `@allsourcedev/client` lives in the `all-source`
> monorepo under `sdks/typescript`. Git installers can't pull a sub-directory of
> a monorepo — that's why `bun add github:…#workspace=@allsourcedev/client` and
> `#path:sdks/typescript` return 404. Install from the npm registry (above).

## Authentication

AllSource uses API keys (signed JWTs). Get one from your dashboard at
[all-source.xyz](https://all-source.xyz) → **Settings → API Keys**. The key is
sent as `Authorization: Bearer <key>`, which the gateway requires, and also as
`X-API-Key`; the SDK does this for you when you pass `apiKey` to the constructor.

Store the key in an env var rather than hard-coding it:

```typescript
import { AllSourceClient } from "@allsourcedev/client";

const client = new AllSourceClient({
  baseUrl: "https://api.all-source.xyz", // or your self-hosted gateway
  apiKey: process.env.ALLSOURCE_API_KEY!,
});
```

The tenant is taken from the key — there is no separate tenant argument.

### Registered client keys (ES256)

Registering a key, computing its kid and rotating it: [the registered client keys guide](https://github.com/all-source-os/all-source/blob/main/docs/guides/REGISTERED_CLIENT_KEYS.md).

A service that talks to the hosted gateway can authenticate with a P-256
keypair instead of a long-lived API key. An operator registers the public key
on your tenant; the SDK then signs a 60-second ES256 assertion for each
request. Background:
[the decision record](../../docs/decisions/2026-11-07-tenant-clients-registered-es256-keys.md).

1. Generate a PKCS#8 P-256 private key on the client's machine and keep it in
   your secret store:

   ```bash
   openssl ecparam -name prime256v1 -genkey -noout | openssl pkcs8 -topk8 -nocrypt -out client-key.pem
   ```

2. Ask an AllSource operator to register the public JWK (`kty`, `crv`, `x`,
   `y`) for your tenant and client id. Registration is operator-only
   (`POST /api/v1/admin/tenants/:id/client-keys`); the answer carries the
   `kid`, the key's RFC 7638 thumbprint. Procedure and rotation:
   [CLIENT_KEY_ROTATION.md](../../docs/runbooks/CLIENT_KEY_ROTATION.md).

3. Pass an `Es256AssertionProvider` as `credentials`:

   ```typescript
   import { AllSourceClient, Es256AssertionProvider } from "@allsourcedev/client";

   const credentials = await Es256AssertionProvider.fromPkcs8Pem(
     process.env.ALLSOURCE_CLIENT_SIGNING_KEY!, // PEM text
     {
       clientId: process.env.ALLSOURCE_CLIENT_ID!,
       tenant: process.env.ALLSOURCE_TENANT_ID!,
       kid: process.env.ALLSOURCE_CLIENT_KID!,
       audience: "https://api.all-source.xyz", // the gateway's ALLSOURCE_ASSERTION_AUDIENCE
     }
   );

   const client = new AllSourceClient({ baseUrl: "https://api.all-source.xyz", credentials });
   ```

`fromPkcs8Pem` refuses a key that is not a P-256 PKCS#8 PEM, or an empty
field, when it is called, so a misconfigured service fails at boot rather than
on its first request. Each assertion lives 60 seconds and is reused until 15
seconds before it expires; the gateway allows 30 seconds of clock skew, so keep
the client's clock synced. Role and scopes come from the registration, not the
token. The provider uses WebCrypto: Node 19+, Bun and browsers have it
globally; on Node 18 pass `crypto: require("node:crypto").webcrypto`.

`credentials` also accepts your own provider: a function, or an object with a
`bearer()` method, returning the token (without `Bearer `) or a promise of it.
The client asks it on every attempt, retries included, and sends only
`Authorization: Bearer <token>`. If it throws, the request fails with
`CredentialError` and is not retried.

### Legacy key sunset

A deployment can set `ALLSOURCE_LEGACY_KEY_SUNSET`. After that instant, keys
minted before managed registration (the old onboarding flow) are refused with
`401` and reason `legacy_key_sunset`. Dashboard keys and registered client keys
keep working. `err.isLegacyKeySunset()` detects it; the fix is a registered
client key or a dashboard key.

## Quick Start

```typescript
import { AllSourceClient } from "@allsourcedev/client";

const client = new AllSourceClient({
  baseUrl: "https://api.all-source.xyz",
  apiKey: process.env.ALLSOURCE_API_KEY!,
});

// Ingest an event — returns the stored event
const event = await client.ingestEvent({
  event_type: "user.signup",
  entity_id: "user-abc-123",
  payload: { email: "jane@example.com", plan: "pro" },
  metadata: { source: "web", ip: "1.2.3.4" },
});
console.log("Ingested:", event.id);

// Query events back
const { events, count } = await client.queryEvents({
  entity_id: "user-abc-123",
  limit: 50,
});
console.log(`Found ${count} events`);

// Health check (unauthenticated)
const health = await client.getHealth();
console.log("Status:", health.status);
```

## API

### `new AllSourceClient(config)`

| Option           | Type       | Required | Default | Description                          |
|------------------|------------|----------|---------|--------------------------------------|
| `baseUrl`        | `string`   | Yes      | —       | AllSource gateway URL                |
| `apiKey`         | `string`   | One of   | —       | Static API key (sent as Bearer and `X-API-Key`) |
| `credentials`    | `CredentialProvider` | One of | — | Asked for a bearer on every attempt; sent as Bearer only |
| `timeout`        | `number`   | No       | `30000` | Request timeout in milliseconds      |
| `retry`          | `object`   | No       | —       | Retry/backoff overrides              |
| `circuitBreaker` | `object`   | No       | —       | Circuit-breaker overrides            |
| `fetch`          | `function` | No       | global  | Custom `fetch` implementation        |

### `client.ingestEvent(event)` → `Event`

Ingest a single event and return the stored event (`id`, `timestamp`, …).

```typescript
const stored = await client.ingestEvent({
  event_type: "order.placed",
  entity_id: "order-456",
  payload: { total: 99.99, currency: "USD" },
  metadata: { source: "checkout" }, // optional, preserved
});
```

### `client.ingestBatch(events)` → `{ count, events }`

Ingest many events in one request.

### `client.queryEvents(params?)` → `{ events, count }`

Query events with optional filters.

| Param        | Type     | Description                       |
|--------------|----------|-----------------------------------|
| `entity_id`  | `string` | Filter by entity ID               |
| `event_type` | `string` | Filter by event type              |
| `limit`      | `number` | Max events to return              |
| `offset`     | `number` | Number of events to skip          |
| `since`      | `string` | Start time, inclusive (ISO 8601)  |
| `until`      | `string` | End time, inclusive (ISO 8601)    |

### `client.queryAndFold(params, folder)` → `S | undefined`

Query events and fold them into a derived state with an `EventFolder<S>`.

### Latest state without history

These calls follow the
[latest-state read contract](../../docs/contracts/latest-state/README.md).
Conditional reads take `ifNoneMatch` (an earlier result's `etag`) and return a
`NotModified` instance when nothing changed, so an unchanged read costs no body.

```typescript
import { NotModified, ViewRowNotFound } from "@allsourcedev/client";

const state = await client.entityState("skill:abc"); // no history[] unless { history: true }
const again = await client.entityState("skill:abc", { ifNoneMatch: state.etag ?? undefined });
if (again instanceof NotModified) {
  // keep using `state`
}
```

`queryEvents(params, { ifNoneMatch })` works the same way, and its response
carries `entity_version` when the query filters by `entity_id`.

### Views

A view is a declarative read model: you define it, AllSource folds it from your
events, and you read rows by key.

```typescript
await client.defineView("ops_status", {
  source: { prefixes: ["ops.probe."] },
  key: "{payload.probe}:{day:timestamp}",
  ops: [
    { on: "ops.probe.observed", when: { "payload.ok": true }, increment: { ok: 1 } },
    { on: "ops.probe.observed", max: { last_seen_at: "timestamp" } },
  ],
});

const row = await client.latest("ops_status", "probe-a:2026-10-27");
if (row instanceof ViewRowNotFound) console.log("no row yet; view ready:", row.ready);

for await (const r of client.latestMany("ops_status", { prefix: "probe-a:" })) {
  console.log(r.key, r.state);
}
```

`getView(name)`, `listViews()` and `deleteView(name)` manage definitions.
`defineView` throws `InvalidViewError` (with `detail`) for a rejected definition.

### Keeping a local copy in sync

`sync(view, { checkpoint, wait })` reads every snapshot page, then follows the
view's changes. Store each `checkpoint` item and pass it back to resume without
a snapshot. A `reset` item means the view was rebuilt: drop your local rows,
because the snapshot that follows re-sends only live ones. `429`/`503` with
`Retry-After` are waited out and the request repeated.

```typescript
const rows = new Map<string, unknown>();
for await (const item of client.sync("ops_status", { checkpoint: saved, wait: 20 })) {
  if (item.type === "reset") rows.clear();
  if (item.type === "upsert") rows.set(item.key, item.state);
  if (item.type === "delete") rows.delete(item.key);
  if (item.type === "checkpoint") await save(item.checkpoint);
}
```

`viewEntities(view, { prefix, after, limit })` and
`viewChanges(view, { after, wait })` are the single-request calls underneath.

### Event feed and read models

For consumers that run their own fold:

```typescript
let cursor: FeedCursor | undefined; // import type { FeedCursor } from "@allsourcedev/client"
const page = await client.feed({ cursor, prefixes: ["skill."], limit: 100 });
cursor = page.next;

const model = client.readModel("skills");
const current = await model.page({ prefix: "skill:" });
await model.commit({
  expectedRevision: current.revision,
  checkpoint: page.next,
  ready: page.caught_up,
  changes: { "skill:abc": { status: "retired" }, "skill:old": null }, // null deletes
});
```

`feed` throws `EpochChangedError` when the feed generation changed: rebuild
from position 0 of `error.currentEpoch` and commit with `reset: true`. A stale
`expectedRevision` throws `VersionConflictError` and is not retried. Committing
needs a key with `projections:write`.

### Prime projections

`definePrimeProjection(entityType, fieldPolicies)`,
`projectNode(nodeId)`, `listPrimeProjections()`,
`nodeFieldProvenance(nodeId, field)` — declarative projections with per-field
merge policies and provenance.

### `client.getHealth()` → `HealthResponse`

Service health. Unauthenticated.

### Reliability

Every request goes through exponential-backoff retries (on 408/429/5xx and
network errors) and a circuit breaker. Both are configurable via the constructor.

### Error handling

API errors throw `AllSourceError` with `status`, `body` and, for an auth
refusal, `reason`:

```typescript
import { AllSourceClient, AllSourceError, CredentialError } from "@allsourcedev/client";

try {
  await client.ingestEvent({ /* … */ });
} catch (err) {
  if (err instanceof CredentialError) {
    console.error("Could not produce a credential:", err.message); // not retried
  } else if (err instanceof AllSourceError) {
    console.error(`API error ${err.status} (${err.reason ?? "no reason"}):`, err.body);
    if (err.isLegacyKeySunset()) console.error("Register a client key or create a dashboard key.");
  }
}
```

| `reason` | Meaning |
|---|---|
| `legacy_key_sunset` | a key minted before managed registration, refused after the deployment's sunset |
| `asserted_signature` | malformed assertion or `kid`, or the signature does not match the registered key |
| `asserted_audience` | `aud` differs from the gateway's configured audience |
| `asserted_skew` | `iat` more than 30 s ahead of the gateway, or `exp - iat` over 60 s |
| `asserted_expired` | the assertion expired more than 30 s ago |
| `asserted_unregistered` | no registration for this tenant and `kid`, or `clientId`/tenant mismatch |
| `asserted_revoked` | the `kid` was revoked; rotate to a new key |
| `asserted_unavailable` | `503`: the gateway cannot read registrations; retried automatically |

Full table: [CLIENT_KEY_ROTATION.md](../../docs/runbooks/CLIENT_KEY_ROTATION.md#reading-refusals).

## License

MIT
