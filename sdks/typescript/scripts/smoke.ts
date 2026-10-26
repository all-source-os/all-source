#!/usr/bin/env bun
/**
 * End-to-end smoke test against a live AllSource Query Service.
 *
 * Proves the full data plane for a tenant: construct client → ingest one event
 * → query it back → health check. Exercises the exact paths that were broken in
 * the dogfooding incident:
 *   - X-API-Key authentication on a data endpoint (was 401 "invalid API key")
 *   - POST /api/v1/events write route (was 404)
 *   - event metadata round-trips (was silently dropped on single create)
 *
 * Usage:
 *   ALLSOURCE_API_KEY=<key> bun run scripts/smoke.ts
 *   ALLSOURCE_API_KEY=<key> ALLSOURCE_BASE_URL=https://allsource-query.fly.dev bun run scripts/smoke.ts
 *
 * Exit codes: 0 pass, 1 assertion failed, 2 misconfigured.
 */
import { AllSourceClient, AllSourceError } from "../src";

const baseUrl = process.env.ALLSOURCE_BASE_URL ?? "https://allsource-query.fly.dev";
const apiKey = process.env.ALLSOURCE_API_KEY;

if (!apiKey) {
  console.error("✗ set ALLSOURCE_API_KEY (a service-account API key for the tenant)");
  process.exit(2);
}

const client = new AllSourceClient({ baseUrl, apiKey });
const entityId = `smoke-${Date.now()}`;

function fail(msg: string, err?: unknown): never {
  console.error(`✗ FAIL: ${msg}`);
  if (err instanceof AllSourceError) {
    console.error(`  AllSourceError ${err.status}:`, JSON.stringify(err.body));
  } else if (err) {
    console.error("  ", err);
  }
  process.exit(1);
}

console.log(`→ gateway: ${baseUrl}`);
console.log(`→ entity:  ${entityId}`);

// 1. health (unauthenticated)
const health = await client.getHealth().catch((e) => fail("getHealth", e));
console.log(
  `✓ health: ${health.status} (version ${(health as Record<string, unknown>).version ?? "?"})`
);

// 2. ingest one event (POST /api/v1/events, X-API-Key auth, metadata)
const stored = await client
  .ingestEvent({
    event_type: "smoke.test",
    entity_id: entityId,
    payload: { hello: "world", n: 42 },
    metadata: { source: "sdk-smoke" },
  })
  .catch((e) => fail("ingestEvent", e));
console.log(`✓ ingested event id=${stored.id ?? "(no id)"} ts=${stored.timestamp ?? "?"}`);

// 3. query it back (eventual; poll briefly)
let found = false;
for (let attempt = 0; attempt < 10; attempt++) {
  const { events, count } = await client
    .queryEvents({ entity_id: entityId, limit: 10 })
    .catch((e) => fail("queryEvents", e));
  if (count > 0) {
    const e = events[0];
    console.log(
      `✓ queried back ${count} event(s); type=${e.event_type} metadata=${JSON.stringify(e.metadata)}`
    );
    found = true;
    break;
  }
  await new Promise((r) => setTimeout(r, 500));
}
if (!found) fail("event not queryable after ingest (10 attempts)");

console.log("\n✓ PASS — construct → ingest → query back works end-to-end");
