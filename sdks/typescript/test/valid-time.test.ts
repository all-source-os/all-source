import { describe, expect, test } from "bun:test";
import { AllSourceClient } from "../src";

function recordingClient(body: unknown) {
  const calls: { url: string; init: RequestInit }[] = [];
  const fetch = ((input: string | URL | Request, init?: RequestInit) => {
    calls.push({ url: String(input), init: init ?? {} });
    return Promise.resolve(
      new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } })
    );
  }) as typeof globalThis.fetch;
  const client = new AllSourceClient({
    baseUrl: "http://core.test",
    apiKey: "test-key",
    fetch,
    retry: { maxRetries: 0 },
  });
  return { client, calls };
}

describe("valid time on ingest", () => {
  test("sends valid_from and valid_to in the body when given", async () => {
    const { client, calls } = recordingClient({
      data: {
        event_id: "e1",
        timestamp: "2026-11-04T10:00:00Z",
        version: 1,
        valid_from: "2026-09-01T00:00:00Z",
        recorded_at: "2026-11-04T10:00:00Z",
      },
    });
    const created = await client.ingestEvent({
      event_type: "skill.published",
      entity_id: "skill:abc",
      payload: {},
      valid_from: "2026-09-01T00:00:00Z",
      valid_to: "2026-09-30T00:00:00Z",
    });
    const sent = JSON.parse(calls[0]!.init.body as string);
    expect(sent.valid_from).toBe("2026-09-01T00:00:00Z");
    expect(sent.valid_to).toBe("2026-09-30T00:00:00Z");
    expect(created.valid_from).toBe("2026-09-01T00:00:00Z");
    expect(created.recorded_at).toBe("2026-11-04T10:00:00Z");
  });

  test("omits valid time from the body when not given", async () => {
    const { client, calls } = recordingClient({
      data: { event_id: "e1", timestamp: "2026-11-04T10:00:00Z" },
    });
    const created = await client.ingestEvent({ event_type: "a.b", entity_id: "e", payload: {} });
    const sent = JSON.parse(calls[0]!.init.body as string);
    expect("valid_from" in sent).toBe(false);
    expect("valid_to" in sent).toBe(false);
    expect(created.valid_from).toBeUndefined();
  });
});

describe("valid time on reads", () => {
  test("entityState sends as_of_valid next to as_of", async () => {
    const { client, calls } = recordingClient({
      entity_id: "skill:abc",
      current_state: {},
      event_count: 0,
      last_updated: null,
      as_of: "2026-10-01T00:00:00Z",
      as_of_valid: "2026-09-15T00:00:00Z",
      version: 0,
    });
    await client.entityState("skill:abc", {
      asOf: "2026-10-01T00:00:00Z",
      asOfValid: "2026-09-15T00:00:00Z",
    });
    const url = new URL(calls[0]!.url);
    expect(url.searchParams.get("as_of")).toBe("2026-10-01T00:00:00Z");
    expect(url.searchParams.get("as_of_valid")).toBe("2026-09-15T00:00:00Z");
  });

  test("queryEvents sends each valid-time bound under its own key", async () => {
    const { client, calls } = recordingClient({ events: [], count: 0 });
    await client.queryEvents({
      as_of: "2026-10-01T00:00:00Z",
      since: "2026-01-01T00:00:00Z",
      as_of_valid: "2026-09-15T00:00:00Z",
      since_valid: "2026-09-01T00:00:00Z",
      until_valid: "2026-09-30T00:00:00Z",
    });
    const url = new URL(calls[0]!.url);
    expect(url.searchParams.get("as_of")).toBe("2026-10-01T00:00:00Z");
    expect(url.searchParams.get("since")).toBe("2026-01-01T00:00:00Z");
    expect(url.searchParams.get("as_of_valid")).toBe("2026-09-15T00:00:00Z");
    expect(url.searchParams.get("since_valid")).toBe("2026-09-01T00:00:00Z");
    expect(url.searchParams.get("until_valid")).toBe("2026-09-30T00:00:00Z");
    expect(url.searchParams.has("until")).toBe(false);
  });

  test("entityState without valid time sends no as_of_valid key", async () => {
    const { client, calls } = recordingClient({
      entity_id: "skill:abc",
      current_state: {},
      event_count: 0,
      last_updated: null,
      as_of: null,
      version: 0,
    });
    await client.entityState("skill:abc");
    expect(new URL(calls[0]!.url).searchParams.has("as_of_valid")).toBe(false);
  });
});
