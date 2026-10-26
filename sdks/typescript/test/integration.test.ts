/**
 * Integration tests for the AllSource TypeScript SDK.
 *
 * Requires a running AllSource Core instance.
 *
 * Run:
 *   ALLSOURCE_TEST_CORE_URL=http://localhost:3280 \
 *   ALLSOURCE_TEST_API_KEY=test-key \
 *     bun test test/integration.test.ts
 */

import { beforeAll, describe, expect, test } from "bun:test";
import { AllSourceClient, AllSourceError } from "../src";

const CORE_URL = process.env.ALLSOURCE_TEST_CORE_URL;
const API_KEY = process.env.ALLSOURCE_TEST_API_KEY ?? "test-key";

function uniqueEntity(prefix: string): string {
  return `${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function skipUnlessCore() {
  if (!CORE_URL) {
    console.log("SKIPPED: set ALLSOURCE_TEST_CORE_URL to run integration tests");
    return true;
  }
  return false;
}

function makeClient(): AllSourceClient {
  return new AllSourceClient({ baseUrl: CORE_URL!, apiKey: API_KEY });
}

describe("Integration: Health", () => {
  test("core health returns healthy", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const health = await client.getHealth();
    expect(health.status).toBe("healthy");
  });
});

describe("Integration: Single Event", () => {
  test("ingest and query round-trip", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const entity = uniqueEntity("ts-sdk-single");

    // Ingest
    const resp = await client.ingestEvent({
      event_type: "ts.test.created",
      entity_id: entity,
      payload: { test: true, value: 42 },
      metadata: { source: "ts-sdk-integration-test" },
    });

    expect(resp.event_id).toBeTruthy();
    expect(resp.timestamp).toBeTruthy();

    // Query back
    const result = await client.queryEvents({ entity_id: entity });
    expect(result.count).toBe(1);
    expect(result.events.length).toBe(1);
    expect(result.events[0].entity_id).toBe(entity);
    expect(result.events[0].event_type).toBe("ts.test.created");
    expect(result.events[0].payload.value).toBe(42);
  });

  test("ingest with no metadata", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const entity = uniqueEntity("ts-sdk-no-meta");

    const resp = await client.ingestEvent({
      event_type: "ts.test.minimal",
      entity_id: entity,
      payload: {},
    });

    expect(resp.event_id).toBeTruthy();
  });
});

describe("Integration: Batch Ingest", () => {
  test("batch ingest 5 events", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const entity = uniqueEntity("ts-sdk-batch");

    const events = Array.from({ length: 5 }, (_, i) => ({
      event_type: `ts.test.batch.item${i}`,
      entity_id: entity,
      payload: { index: i },
    }));

    const resp = await client.ingestBatch(events);
    expect(resp.total).toBe(5);
    expect(resp.ingested).toBe(5);
    expect(resp.events.length).toBe(5);

    // Unique IDs
    const ids = new Set(resp.events.map((e) => e.event_id));
    expect(ids.size).toBe(5);

    // Query back
    const result = await client.queryEvents({ entity_id: entity });
    expect(result.count).toBe(5);
  });
});

describe("Integration: Query Filters", () => {
  test("query by event_type", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const entity = uniqueEntity("ts-sdk-type");
    const eventType = `ts.test.typed.${Date.now()}`;

    for (let i = 0; i < 3; i++) {
      await client.ingestEvent({
        event_type: eventType,
        entity_id: entity,
        payload: { seq: i },
      });
    }

    const result = await client.queryEvents({ event_type: eventType });
    expect(result.count).toBeGreaterThanOrEqual(3);
    for (const event of result.events) {
      expect(event.event_type).toBe(eventType);
    }
  });

  test("query with limit", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const entity = uniqueEntity("ts-sdk-limit");

    for (let i = 0; i < 5; i++) {
      await client.ingestEvent({
        event_type: "ts.test.limit",
        entity_id: entity,
        payload: { seq: i },
      });
    }

    const result = await client.queryEvents({
      entity_id: entity,
      limit: 2,
    });
    expect(result.events.length).toBe(2);
  });

  test("query nonexistent entity returns empty", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const result = await client.queryEvents({
      entity_id: "nonexistent-ts-sdk-99999",
    });
    expect(result.count).toBe(0);
    expect(result.events.length).toBe(0);
  });
});

describe("Integration: Error Handling", () => {
  test("connection refused", async () => {
    const client = new AllSourceClient({
      baseUrl: "http://127.0.0.1:19999",
      apiKey: "key",
      timeout: 2000,
    });

    try {
      await client.getHealth();
      expect(true).toBe(false); // should not reach
    } catch (e) {
      expect(e).toBeDefined();
    }
  });
});

describe("Integration: Data Integrity", () => {
  test("payload fidelity", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const entity = uniqueEntity("ts-sdk-fidelity");

    const complexPayload = {
      string: "hello world",
      number: 42,
      float: 3.14159,
      boolean: true,
      null_val: null,
      nested: {
        array: [1, 2, 3],
        deep: { key: "value" },
      },
    };

    await client.ingestEvent({
      event_type: "ts.test.fidelity",
      entity_id: entity,
      payload: complexPayload,
    });

    const result = await client.queryEvents({ entity_id: entity });
    expect(result.count).toBe(1);

    const stored = result.events[0].payload;
    expect(stored.string).toBe("hello world");
    expect(stored.number).toBe(42);
    expect(stored.boolean).toBe(true);
    expect(stored.null_val).toBeNull();
    expect((stored.nested as any).array[1]).toBe(2);
    expect((stored.nested as any).deep.key).toBe("value");
  });

  test("metadata preserved", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const entity = uniqueEntity("ts-sdk-metadata");

    await client.ingestEvent({
      event_type: "ts.test.metadata",
      entity_id: entity,
      payload: { data: 1 },
      metadata: {
        correlation_id: "corr-123",
        source: "integration-test",
      },
    });

    const result = await client.queryEvents({ entity_id: entity });
    expect(result.count).toBe(1);
    expect(result.events[0].metadata.correlation_id).toBe("corr-123");
    expect(result.events[0].metadata.source).toBe("integration-test");
  });
});

describe("Integration: Event Ordering", () => {
  test("events returned in chronological order", async () => {
    if (skipUnlessCore()) return;
    const client = makeClient();
    const entity = uniqueEntity("ts-sdk-order");

    for (let i = 0; i < 5; i++) {
      await client.ingestEvent({
        event_type: "ts.test.ordered",
        entity_id: entity,
        payload: { seq: i },
      });
      await new Promise((r) => setTimeout(r, 10));
    }

    const result = await client.queryEvents({ entity_id: entity });
    expect(result.count).toBe(5);

    for (let i = 0; i < result.events.length; i++) {
      expect(result.events[i].payload.seq).toBe(i);
    }
  });
});
