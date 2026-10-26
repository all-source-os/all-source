import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import type { EventFolder } from "../src";
import {
  AllSourceClient,
  AllSourceError,
  CircuitOpenError,
  NotModified,
  ViewRowNotFound,
} from "../src";

const MOCK_BASE_URL = "https://allsource-query.example.com";
const MOCK_API_KEY = "test-api-key-123";

function createClient(overrides?: {
  baseUrl?: string;
  apiKey?: string;
  timeout?: number;
  fetch?: typeof globalThis.fetch;
  retry?: { maxRetries?: number; baseDelay?: number; backoffFactor?: number; maxDelay?: number };
  circuitBreaker?: { threshold?: number; recoveryTimeout?: number };
}) {
  return new AllSourceClient({
    baseUrl: overrides?.baseUrl ?? MOCK_BASE_URL,
    apiKey: overrides?.apiKey ?? MOCK_API_KEY,
    ...overrides,
  });
}

// Mock fetch globally
const originalFetch = globalThis.fetch;
let mockFetch: ReturnType<typeof mock>;

beforeEach(() => {
  mockFetch = mock(() => Promise.resolve(new Response(JSON.stringify({}), { status: 200 })));
  globalThis.fetch = mockFetch as unknown as typeof fetch;
});

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe("AllSourceClient constructor", () => {
  test("throws if baseUrl is empty", () => {
    expect(() => createClient({ baseUrl: "" })).toThrow("baseUrl is required");
  });

  test("throws if apiKey is empty", () => {
    expect(() => createClient({ apiKey: "" })).toThrow("apiKey is required");
  });

  test("strips trailing slashes from baseUrl", async () => {
    const client = createClient({ baseUrl: "https://example.com///" });
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ status: "ok" }), { status: 200 }))
    );
    await client.getHealth();
    const url = (mockFetch.mock.calls[0] as unknown[])[0] as string;
    expect(url).toBe("https://example.com/health");
  });
});

describe("getHealth", () => {
  test("sends GET /health with API key header", async () => {
    const client = createClient();
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ status: "ok" }), { status: 200 }))
    );

    const result = await client.getHealth();

    expect(result).toEqual({ status: "ok" });
    expect(mockFetch).toHaveBeenCalledTimes(1);

    const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${MOCK_BASE_URL}/health`);
    expect(options.method).toBe("GET");
    expect((options.headers as Record<string, string>)["X-API-Key"]).toBe(MOCK_API_KEY);
  });

  test("sends the key as a Bearer token, the only header the gateway reads", async () => {
    const client = createClient();
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ status: "ok" }), { status: 200 }))
    );

    await client.getHealth();

    const [, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect((options.headers as Record<string, string>).Authorization).toBe(
      `Bearer ${MOCK_API_KEY}`
    );
  });
});

describe("ingestEvent", () => {
  test("sends POST /api/v1/events with event body", async () => {
    const client = createClient();
    const event = {
      event_type: "user.signup",
      entity_id: "user-123",
      payload: { email: "test@example.com" },
      metadata: { source: "sdk-test" },
    };
    // The gateway wraps the create ack in `{ data }` and keys the id as
    // `event_id` (EventResponse); the SDK normalizes it to `id`.
    const ack = {
      event_id: "evt-1",
      timestamp: "2026-02-16T00:00:00Z",
      version: 1,
    };

    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ data: ack }), { status: 201 }))
    );

    const result = await client.ingestEvent(event);

    expect(result.id).toBe("evt-1");
    expect(result.timestamp).toBe("2026-02-16T00:00:00Z");
    expect(result.version).toBe(1);

    const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${MOCK_BASE_URL}/api/v1/events`);
    expect(options.method).toBe("POST");
    expect((options.headers as Record<string, string>)["Content-Type"]).toBe("application/json");
    expect(JSON.parse(options.body as string)).toEqual(event);
  });
});

describe("ingestBatch", () => {
  test("posts to /api/v1/events/batch and normalizes the created events", async () => {
    const client = createClient();
    const events = [
      { event_type: "a", entity_id: "e1", payload: {} },
      { event_type: "b", entity_id: "e2", payload: {} },
    ];
    // Gateway batch response: { data: [...acks], count }, acks keyed as event_id.
    const body = {
      data: [
        { event_id: "evt-1", timestamp: "2026-02-16T00:00:00Z", version: 1 },
        { event_id: "evt-2", timestamp: "2026-02-16T00:00:01Z", version: 2 },
      ],
      count: 2,
    };

    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify(body), { status: 201 }))
    );

    const result = await client.ingestBatch(events);

    expect(result.count).toBe(2);
    expect(result.events.map((e) => e.id)).toEqual(["evt-1", "evt-2"]);

    const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${MOCK_BASE_URL}/api/v1/events/batch`);
    expect(options.method).toBe("POST");
    expect(JSON.parse(options.body as string)).toEqual({ events });
  });
});

describe("queryEvents", () => {
  test("sends GET /api/v1/events/query with no params", async () => {
    const client = createClient();
    const response = { events: [], count: 0 };

    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify(response), { status: 200 }))
    );

    const result = await client.queryEvents();

    expect(result).toEqual(response);
    const [url] = mockFetch.mock.calls[0] as [string];
    expect(url).toBe(`${MOCK_BASE_URL}/api/v1/events/query`);
  });

  test("sends query params when provided", async () => {
    const client = createClient();
    const response = { events: [], count: 0 };

    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify(response), { status: 200 }))
    );

    await client.queryEvents({ entity_id: "user-123", limit: 10, event_type: "user.signup" });

    const [url] = mockFetch.mock.calls[0] as [string];
    const parsed = new URL(url);
    expect(parsed.searchParams.get("entity_id")).toBe("user-123");
    expect(parsed.searchParams.get("limit")).toBe("10");
    expect(parsed.searchParams.get("event_type")).toBe("user.signup");
  });

  test("sends since/until params (not start_time/end_time)", async () => {
    const client = createClient();
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ events: [], count: 0 }), { status: 200 }))
    );

    await client.queryEvents({ since: "2026-01-01T00:00:00Z", until: "2026-02-01T00:00:00Z" });

    const [url] = mockFetch.mock.calls[0] as [string];
    const parsed = new URL(url);
    expect(parsed.searchParams.get("since")).toBe("2026-01-01T00:00:00Z");
    expect(parsed.searchParams.get("until")).toBe("2026-02-01T00:00:00Z");
    expect(parsed.searchParams.has("start_time")).toBe(false);
    expect(parsed.searchParams.has("end_time")).toBe(false);
  });

  test("omits undefined params", async () => {
    const client = createClient();
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ events: [], count: 0 }), { status: 200 }))
    );

    await client.queryEvents({ entity_id: "user-123", limit: undefined });

    const [url] = mockFetch.mock.calls[0] as [string];
    const parsed = new URL(url);
    expect(parsed.searchParams.get("entity_id")).toBe("user-123");
    expect(parsed.searchParams.has("limit")).toBe(false);
  });
});

describe("listPrimeProjections", () => {
  test("sends GET /api/v1/prime/projections and unwraps data", async () => {
    const client = createClient();
    const projections = [
      { entity_type: "user", field_policies: { name: "last_write", role: "highest_priority" } },
    ];

    mockFetch.mockImplementation(() =>
      Promise.resolve(
        new Response(JSON.stringify({ data: projections, count: 1 }), { status: 200 })
      )
    );

    const result = await client.listPrimeProjections();

    expect(result).toEqual(projections);
    expect(result.length).toBe(1);

    const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${MOCK_BASE_URL}/api/v1/prime/projections`);
    expect(options.method).toBe("GET");
  });
});

describe("definePrimeProjection", () => {
  test("sends POST /api/v1/prime/projections with snake_case body and unwraps data", async () => {
    const client = createClient();
    const ack = { entity_type: "user", persisted: true };

    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ data: ack }), { status: 201 }))
    );

    const fieldPolicies = { name: "last_write", role: "most_specific" };
    const result = await client.definePrimeProjection("user", fieldPolicies);

    expect(result).toEqual(ack);

    const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${MOCK_BASE_URL}/api/v1/prime/projections`);
    expect(options.method).toBe("POST");
    const sent = JSON.parse(options.body as string);
    expect(sent.entity_type).toBe("user");
    expect(sent.field_policies).toEqual(fieldPolicies);
  });
});

describe("projectNode", () => {
  test("sends POST /api/v1/prime/nodes/{id}/project and unwraps data", async () => {
    const client = createClient();
    const snapshot = {
      entity_type: "user",
      fields: { name: "Ada" },
      observation_count: 3,
    };

    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ data: snapshot }), { status: 200 }))
    );

    const result = await client.projectNode("user:abc");

    expect(result).toEqual(snapshot);

    const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${MOCK_BASE_URL}/api/v1/prime/nodes/user:abc/project`);
    expect(options.method).toBe("POST");
  });
});

describe("nodeFieldProvenance", () => {
  test("sends GET provenance path and unwraps data", async () => {
    const client = createClient();
    const provenance = {
      field: "name",
      value: "Ada",
      source_event_id: "evt-1",
      source_event_at: "2026-02-16T00:00:00Z",
      merge_policy_applied: "last_write",
    };

    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ data: provenance }), { status: 200 }))
    );

    const result = await client.nodeFieldProvenance("user:abc", "name");

    expect(result).toEqual(provenance);

    const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${MOCK_BASE_URL}/api/v1/prime/nodes/user:abc/fields/name/provenance`);
    expect(options.method).toBe("GET");
  });

  test("rejects with AllSourceError (404) when no provenance exists", async () => {
    const client = createClient({ retry: { maxRetries: 0 } });
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ error: "not found" }), { status: 404 }))
    );

    try {
      await client.nodeFieldProvenance("user:abc", "missing");
      expect(true).toBe(false); // should not reach
    } catch (err) {
      expect(err).toBeInstanceOf(AllSourceError);
      expect((err as AllSourceError).isNotFound()).toBe(true);
    }
  });
});

describe("error handling", () => {
  test("throws AllSourceError on non-2xx response", async () => {
    const client = createClient({ retry: { maxRetries: 0 } });
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ error: "Unauthorized" }), { status: 401 }))
    );

    try {
      await client.getHealth();
      expect(true).toBe(false); // should not reach
    } catch (err) {
      expect(err).toBeInstanceOf(AllSourceError);
      const error = err as AllSourceError;
      expect(error.status).toBe(401);
      expect(error.body).toEqual({ error: "Unauthorized" });
    }
  });

  test("AllSourceError includes status text", async () => {
    const client = createClient({ retry: { maxRetries: 0 } });
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response("Not Found", { status: 404, statusText: "Not Found" }))
    );

    try {
      await client.getHealth();
      expect(true).toBe(false);
    } catch (err) {
      const error = err as AllSourceError;
      expect(error.message).toContain("404");
      expect(error.message).toContain("Not Found");
    }
  });
});

describe("error helpers", () => {
  test("isUnauthorized", () => {
    const err = new AllSourceError("test", 401);
    expect(err.isUnauthorized()).toBe(true);
    expect(err.isRateLimited()).toBe(false);
  });

  test("isRateLimited", () => {
    const err = new AllSourceError("test", 429);
    expect(err.isRateLimited()).toBe(true);
  });

  test("isNotFound", () => {
    const err = new AllSourceError("test", 404);
    expect(err.isNotFound()).toBe(true);
  });

  test("isForbidden", () => {
    const err = new AllSourceError("test", 403);
    expect(err.isForbidden()).toBe(true);
  });

  test("isServerError", () => {
    expect(new AllSourceError("test", 500).isServerError()).toBe(true);
    expect(new AllSourceError("test", 503).isServerError()).toBe(true);
    expect(new AllSourceError("test", 400).isServerError()).toBe(false);
  });

  test("isRetryable", () => {
    for (const code of [408, 429, 500, 502, 503, 504]) {
      expect(new AllSourceError("test", code).isRetryable()).toBe(true);
    }
    expect(new AllSourceError("test", 400).isRetryable()).toBe(false);
    expect(new AllSourceError("test", 401).isRetryable()).toBe(false);
  });
});

describe("retry logic", () => {
  test("retries on 503 then succeeds", async () => {
    let callCount = 0;
    const customFetch = mock((_url: string | URL | Request, _init?: RequestInit) => {
      callCount++;
      if (callCount <= 2) {
        return Promise.resolve(
          new Response(JSON.stringify({ error: "Service Unavailable" }), { status: 503 })
        );
      }
      return Promise.resolve(new Response(JSON.stringify({ status: "ok" }), { status: 200 }));
    }) as unknown as typeof globalThis.fetch;

    const client = createClient({
      fetch: customFetch,
      retry: { maxRetries: 3, baseDelay: 1, backoffFactor: 1, maxDelay: 1 },
    });

    const result = await client.getHealth();
    expect(result).toEqual({ status: "ok" });
    expect(callCount).toBe(3);
  });

  test("does not retry on 401", async () => {
    let callCount = 0;
    const customFetch = mock((_url: string | URL | Request, _init?: RequestInit) => {
      callCount++;
      return Promise.resolve(
        new Response(JSON.stringify({ error: "Unauthorized" }), { status: 401 })
      );
    }) as unknown as typeof globalThis.fetch;

    const client = createClient({
      fetch: customFetch,
      retry: { maxRetries: 3, baseDelay: 1 },
    });

    try {
      await client.getHealth();
      expect(true).toBe(false);
    } catch (err) {
      expect(err).toBeInstanceOf(AllSourceError);
      expect((err as AllSourceError).status).toBe(401);
    }
    expect(callCount).toBe(1);
  });

  test("retries on network error then succeeds", async () => {
    let callCount = 0;
    const customFetch = mock((_url: string | URL | Request, _init?: RequestInit) => {
      callCount++;
      if (callCount <= 1) {
        return Promise.reject(new TypeError("fetch failed"));
      }
      return Promise.resolve(new Response(JSON.stringify({ status: "ok" }), { status: 200 }));
    }) as unknown as typeof globalThis.fetch;

    const client = createClient({
      fetch: customFetch,
      retry: { maxRetries: 2, baseDelay: 1, backoffFactor: 1, maxDelay: 1 },
    });

    const result = await client.getHealth();
    expect(result).toEqual({ status: "ok" });
    expect(callCount).toBe(2);
  });
});

describe("circuit breaker integration", () => {
  test("opens circuit after threshold failures", async () => {
    let callCount = 0;
    const customFetch = mock((_url: string | URL | Request, _init?: RequestInit) => {
      callCount++;
      return Promise.resolve(
        new Response(JSON.stringify({ error: "Server Error" }), { status: 500 })
      );
    }) as unknown as typeof globalThis.fetch;

    const client = createClient({
      fetch: customFetch,
      retry: { maxRetries: 0, baseDelay: 1 },
      circuitBreaker: { threshold: 3, recoveryTimeout: 60_000 },
    });

    // Fail 3 times to trip the breaker
    for (let i = 0; i < 3; i++) {
      try {
        await client.getHealth();
      } catch {
        // expected
      }
    }

    // Next call should throw CircuitOpenError without making a fetch
    const callCountBefore = callCount;
    try {
      await client.getHealth();
      expect(true).toBe(false);
    } catch (err) {
      expect(err).toBeInstanceOf(CircuitOpenError);
    }
    expect(callCount).toBe(callCountBefore);
  });
});

describe("queryAndFold", () => {
  test("folds events into a state", async () => {
    const events = [
      {
        id: "1",
        event_type: "counter.incremented",
        entity_id: "c1",
        payload: { amount: 5 },
        metadata: {},
        timestamp: "t1",
      },
      {
        id: "2",
        event_type: "counter.incremented",
        entity_id: "c1",
        payload: { amount: 3 },
        metadata: {},
        timestamp: "t2",
      },
      {
        id: "3",
        event_type: "counter.decremented",
        entity_id: "c1",
        payload: { amount: 2 },
        metadata: {},
        timestamp: "t3",
      },
    ];

    const customFetch = mock((_url: string | URL | Request, _init?: RequestInit) => {
      return Promise.resolve(new Response(JSON.stringify({ events, count: 3 }), { status: 200 }));
    }) as unknown as typeof globalThis.fetch;

    const client = createClient({ fetch: customFetch });

    const folder: EventFolder<number> = {
      _total: 0,
      _applied: false,
      apply(event) {
        if (event.event_type === "counter.incremented") {
          this._total += event.payload.amount as number;
          this._applied = true;
          return true;
        }
        if (event.event_type === "counter.decremented") {
          this._total -= event.payload.amount as number;
          this._applied = true;
          return true;
        }
        return false;
      },
      finalize() {
        return this._applied ? this._total : undefined;
      },
    } as EventFolder<number> & { _total: number; _applied: boolean };

    const result = await client.queryAndFold({ entity_id: "c1" }, folder);
    expect(result).toBe(6); // 5 + 3 - 2
  });
});

describe("latest-state reads", () => {
  function json(body: unknown, init: ResponseInit = {}): Promise<Response> {
    return Promise.resolve(new Response(JSON.stringify(body), { status: 200, ...init }));
  }

  test("queryEvents without an ETag adds no etag field", async () => {
    const client = createClient();
    mockFetch.mockImplementation(() => json({ events: [], count: 0 }));

    const result = await client.queryEvents({ entity_id: "e1" });

    expect("etag" in result).toBe(false);
  });

  test("queryEvents sends If-None-Match and returns NotModified on 304", async () => {
    const client = createClient();
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(null, { status: 304, headers: { ETag: '"sha256:aa"' } }))
    );

    const result = await client.queryEvents({ entity_id: "e1" }, { ifNoneMatch: '"sha256:aa"' });

    expect(result).toBeInstanceOf(NotModified);
    const [, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect((options.headers as Record<string, string>)["If-None-Match"]).toBe('"sha256:aa"');
  });

  test("entityState sends history=true and as_of only when asked", async () => {
    const client = createClient();
    mockFetch.mockImplementation(() =>
      json({
        entity_id: "e1",
        current_state: {},
        event_count: 0,
        last_updated: null,
        as_of: "2026-10-01T00:00:00Z",
        version: 0,
        history: [],
      })
    );

    const state = await client.entityState("e1", { history: true, asOf: "2026-10-01T00:00:00Z" });

    const [url] = mockFetch.mock.calls[0] as [string];
    const parsed = new URL(url);
    expect(parsed.searchParams.get("history")).toBe("true");
    expect(parsed.searchParams.get("as_of")).toBe("2026-10-01T00:00:00Z");
    expect(state.history).toEqual([]);
    expect(state.etag).toBeNull();
  });

  test("feed with no events keeps the request cursor as next", async () => {
    const client = createClient();
    mockFetch.mockImplementation(() =>
      json({
        tenant_id: "t1",
        cursor: { epoch: 41, position: 9 },
        head: { epoch: 41, position: 9 },
        events: [],
        positions: [],
        caught_up: true,
      })
    );

    const page = await client.feed({ cursor: { epoch: 41, position: 9 } });

    expect(page.next).toEqual({ epoch: 41, position: 9 });
  });

  test("latest rejects a 404 that is not a missing row", async () => {
    const client = createClient({ retry: { maxRetries: 0 } });
    mockFetch.mockImplementation(() => json({ error: "route not found" }, { status: 404 }));

    const error = await client.latest("ops_status", "k").catch((e: unknown) => e);

    expect(error).toBeInstanceOf(AllSourceError);
    expect(error).not.toBeInstanceOf(ViewRowNotFound);
  });

  test("deleteView sends DELETE and accepts an empty body", async () => {
    const client = createClient();
    mockFetch.mockImplementation(() => Promise.resolve(new Response(null, { status: 204 })));

    await client.deleteView("ops_status");

    const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`${MOCK_BASE_URL}/api/v1/views/ops_status`);
    expect(options.method).toBe("DELETE");
  });

  test("AllSourceError carries Retry-After in milliseconds", async () => {
    const client = createClient({ retry: { maxRetries: 0 } });
    mockFetch.mockImplementation(() =>
      Promise.resolve(new Response(null, { status: 503, headers: { "Retry-After": "2" } }))
    );

    const error = (await client.getHealth().catch((e: unknown) => e)) as AllSourceError;

    expect(error.status).toBe(503);
    expect(error.retryAfterMs).toBe(2000);
  });

  test("retries wait the server's Retry-After instead of the backoff", async () => {
    let calls = 0;
    const customFetch = mock(() => {
      calls++;
      if (calls === 1) {
        return Promise.resolve(
          new Response(null, { status: 429, headers: { "Retry-After": "0" } })
        );
      }
      return json({ status: "ok" });
    }) as unknown as typeof globalThis.fetch;
    const client = createClient({
      fetch: customFetch,
      retry: { maxRetries: 1, baseDelay: 60_000, backoffFactor: 1, maxDelay: 60_000 },
    });

    expect(await client.getHealth()).toEqual({ status: "ok" });
    expect(calls).toBe(2);
  });
});

describe("custom fetch injection", () => {
  test("uses injected fetch function", async () => {
    let called = false;
    const customFetch = mock((_url: string | URL | Request, _init?: RequestInit) => {
      called = true;
      return Promise.resolve(new Response(JSON.stringify({ status: "ok" }), { status: 200 }));
    }) as unknown as typeof globalThis.fetch;

    const client = createClient({ fetch: customFetch });
    await client.getHealth();

    expect(called).toBe(true);
    // Global mock should NOT have been called
    expect(mockFetch).not.toHaveBeenCalled();
  });
});
