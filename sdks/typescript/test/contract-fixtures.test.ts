/**
 * Runs the SDK against every shared fixture in sdks/contract/fixtures, the
 * bodies all four SDKs and Core agree on (docs/contracts/latest-state).
 */

import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import {
  AllSourceClient,
  EpochChangedError,
  InvalidViewError,
  NotModified,
  VersionConflictError,
  type ViewDefinition,
  ViewResetError,
  ViewRowNotFound,
  type ViewSyncItem,
} from "../src";

interface Fixture {
  name: string;
  description: string;
  request: {
    method: string;
    path: string;
    query: Record<string, string>;
    headers: Record<string, string>;
    body: unknown;
  };
  response: {
    status: number;
    headers: Record<string, string>;
    body: unknown;
  };
}

interface RecordedCall {
  url: string;
  init: RequestInit;
  at: number;
}

const FIXTURE_DIR = join(import.meta.dir, "..", "..", "contract", "fixtures");
const BASE_URL = "https://api.example.com";

function loadFixtures(): Map<string, Fixture> {
  const fixtures = new Map<string, Fixture>();
  for (const file of readdirSync(FIXTURE_DIR).filter((f) => f.endsWith(".json"))) {
    const fixture = JSON.parse(readFileSync(join(FIXTURE_DIR, file), "utf8")) as Fixture;
    fixtures.set(file.slice(0, -".json".length), fixture);
  }
  return fixtures;
}

const fixtures = loadFixtures();

function fixture(name: string): Fixture {
  const found = fixtures.get(name);
  if (!found) throw new Error(`missing fixture ${name}`);
  return found;
}

function toResponse(fx: Fixture): Response {
  const { status, headers, body } = fx.response;
  return new Response(body === null ? null : JSON.stringify(body), { status, headers });
}

/** Serves the fixtures' responses in order and records every request. */
function stub(served: Fixture[], onServe?: (index: number) => void) {
  const calls: RecordedCall[] = [];
  const fetch = ((input: string | URL | Request, init?: RequestInit) => {
    const index = calls.length;
    calls.push({ url: String(input), init: init ?? {}, at: Date.now() });
    const fx = served[index];
    if (!fx) return Promise.reject(new Error(`unexpected request ${index}: ${String(input)}`));
    onServe?.(index);
    return Promise.resolve(toResponse(fx));
  }) as typeof globalThis.fetch;
  const client = new AllSourceClient({
    baseUrl: BASE_URL,
    apiKey: "test-key",
    fetch,
    retry: { maxRetries: 0 },
  });
  return { client, calls };
}

function decodePath(path: string): string {
  return path.split("/").map(decodeURIComponent).join("/");
}

function expectRequest(call: RecordedCall | undefined, fx: Fixture): void {
  expect(call).toBeDefined();
  if (!call) return;
  const url = new URL(call.url);
  expect(call.init.method).toBe(fx.request.method);
  expect(decodePath(url.pathname)).toBe(decodePath(fx.request.path));

  const sent: Record<string, string> = {};
  for (const [key, value] of url.searchParams) sent[key] = value;
  expect(sent).toEqual(fx.request.query);

  const headers = Object.fromEntries(
    Object.entries((call.init.headers ?? {}) as Record<string, string>).map(([k, v]) => [
      k.toLowerCase(),
      v,
    ])
  );
  for (const [name, value] of Object.entries(fx.request.headers)) {
    expect(headers[name.toLowerCase()]).toBe(value);
  }

  if (fx.request.body === null) {
    expect(call.init.body).toBeUndefined();
  } else {
    expect(JSON.parse(call.init.body as string)).toEqual(fx.request.body);
  }
}

async function caught(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
  } catch (error) {
    return error;
  }
  throw new Error("expected the call to reject");
}

function etagOf(fx: Fixture): string {
  const etag = fx.response.headers.ETag;
  if (!etag) throw new Error(`${fx.name} has no ETag`);
  return etag;
}

function ifNoneMatchOf(fx: Fixture): string {
  const tag = fx.request.headers["If-None-Match"];
  if (!tag) throw new Error(`${fx.name} sends no If-None-Match`);
  return tag;
}

type Runner = (fx: Fixture) => Promise<void>;

/** One call per fixture: send the request, check what was sent, check the typed result. */
const runners: Record<string, Runner> = {
  async entity_state(fx) {
    const { client, calls } = stub([fx]);
    const result = await client.entityState("skill:abc");
    expectRequest(calls[0], fx);
    expect(result).not.toBeInstanceOf(NotModified);
    expect(result.current_state).toEqual({ name: "x", status: "published" });
    expect(result.version).toBe(12);
    expect(result.history).toBeUndefined();
    expect(result.etag).toBe(etagOf(fx));
  },

  async entity_state_not_modified(fx) {
    const { client, calls } = stub([fx]);
    const result = await client.entityState("skill:abc", { ifNoneMatch: ifNoneMatchOf(fx) });
    expectRequest(calls[0], fx);
    expect(result).toBeInstanceOf(NotModified);
    expect((result as NotModified).etag).toBe(etagOf(fx));
  },

  async event_feed(fx) {
    const { client, calls } = stub([fx]);
    const page = await client.feed({
      cursor: { epoch: 41, position: 0 },
      prefixes: ["skill."],
      limit: 100,
    });
    expectRequest(calls[0], fx);
    expect(page.events.map((e) => e.event_type)).toEqual(["skill.published"]);
    expect(page.caught_up).toBe(true);
    expect(page.next).toEqual({ epoch: 41, position: 1 });
  },

  async event_feed_epoch_changed(fx) {
    const { client, calls } = stub([fx]);
    const error = await caught(client.feed({ cursor: { epoch: 40, position: 17 } }));
    expectRequest(calls[0], fx);
    expect(error).toBeInstanceOf(EpochChangedError);
    expect((error as EpochChangedError).currentEpoch).toBe(41);
    expect((error as EpochChangedError).expectedVersion).toBe(40);
  },

  async events_query_with_entity_version(fx) {
    const { client, calls } = stub([fx]);
    const result = await client.queryEvents({ entity_id: "skill:abc", limit: 1 });
    expectRequest(calls[0], fx);
    expect(result.entity_version).toBe(12);
    expect(result.events).toHaveLength(1);
    expect(result.etag).toBe(etagOf(fx));
  },

  async rate_limited_retry_after(fx) {
    const answer = fixture("view_changes");
    const { client, calls } = stub([fx, answer]);
    const items: ViewSyncItem[] = [];
    for await (const item of client.sync("ops_status", {
      checkpoint: { revision: 61, generation: 2 },
    })) {
      items.push(item);
      break;
    }
    expect(calls).toHaveLength(2);
    expectRequest(calls[0], fx);
    expectRequest(calls[1], fx);
    expect(calls[1]!.url).toBe(calls[0]!.url);
    // Date.now() truncates to whole milliseconds, so a 1000 ms timer can read as 999.
    expect(calls[1]!.at - calls[0]!.at).toBeGreaterThanOrEqual(999);
    expect(items[0]).toEqual({
      type: "upsert",
      key: "probe-a:2026-10-27",
      state: { ok: 1431, failed: 2 },
      version: 62,
    });
  },

  async read_model_commit(fx) {
    const { client, calls } = stub([fx]);
    const result = await client.readModel("skills").commit({
      expectedRevision: 7,
      checkpoint: { epoch: 41, position: 3 },
      ready: true,
      changes: { "skill:abc": { status: "retired" }, "skill:old": null },
    });
    expectRequest(calls[0], fx);
    expect(result).toEqual({ revision: 8 });
  },

  async read_model_commit_conflict(fx) {
    const { client, calls } = stub([fx]);
    const error = await caught(
      client.readModel("skills").commit({
        expectedRevision: 6,
        checkpoint: { epoch: 41, position: 3 },
        ready: true,
        reset: false,
        changes: { "skill:abc": { status: "retired" } },
      })
    );
    expectRequest(calls[0], fx);
    expect(calls).toHaveLength(1);
    expect(error).toBeInstanceOf(VersionConflictError);
    expect(error).not.toBeInstanceOf(EpochChangedError);
    expect((error as VersionConflictError).currentVersion).toBe(7);
  },

  async read_model_page(fx) {
    const { client, calls } = stub([fx]);
    const page = await client.readModel("skills").page({ prefix: "skill:", limit: 100 });
    expectRequest(calls[0], fx);
    expect(page.unchanged).toBe(false);
    expect(page.rows).toEqual({ "skill:abc": { status: "published" } });
    expect(page.content_revision).toBe(7);
    expect(page.checkpoint).toEqual({ epoch: 41, position: 3 });
    expect(page.next).toBeNull();
  },

  async read_model_unchanged(fx) {
    const { client, calls } = stub([fx]);
    const page = await client.readModel("skills").page({ prefix: "skill:", revision: 7 });
    expectRequest(calls[0], fx);
    expect(page.unchanged).toBe(true);
    expect(page.rows).toEqual({});
  },

  async view_changes(fx) {
    const { client, calls } = stub([fx]);
    const page = await client.viewChanges("ops_status", { after: 61 });
    expectRequest(calls[0], fx);
    expect(page.changes.map((c) => [c.key, c.state === null])).toEqual([
      ["probe-a:2026-10-27", false],
      ["probe-z:2026-10-20", true],
    ]);
    expect(page.revision).toBe(62);
  },

  async view_changes_empty_after_wait(fx) {
    const { client, calls } = stub([fx]);
    const page = await client.viewChanges("ops_status", { after: 62, wait: 20 });
    expectRequest(calls[0], fx);
    expect(page.changes).toEqual([]);
    expect(page.caught_up).toBe(true);
    expect(page.revision).toBe(62);
  },

  async view_changes_reset(fx) {
    const { client, calls } = stub([fx]);
    const error = await caught(client.viewChanges("ops_status", { after: 12 }));
    expectRequest(calls[0], fx);
    expect(error).toBeInstanceOf(ViewResetError);
    expect((error as ViewResetError).generation).toBe(3);
  },

  async view_define(fx) {
    const { client, calls } = stub([fx]);
    const info = await client.defineView("ops_status", fx.request.body as ViewDefinition);
    expectRequest(calls[0], fx);
    expect(info.generation).toBe(1);
    expect(info.ready).toBe(false);
    expect(info.checkpoint).toBeNull();
  },

  async view_define_invalid(fx) {
    const { client, calls } = stub([fx]);
    const error = await caught(client.defineView("ops_status", fx.request.body as ViewDefinition));
    expectRequest(calls[0], fx);
    expect(error).toBeInstanceOf(InvalidViewError);
    expect((error as InvalidViewError).detail).toBe("ops must contain 1..64 operations");
  },

  async view_entities_page_1(fx) {
    const { client, calls } = stub([fx]);
    const page = await client.viewEntities("ops_status", { limit: 2 });
    expectRequest(calls[0], fx);
    expect(page.rows).toHaveLength(2);
    expect(page.next).toBe("probe-a:2026-10-27");
  },

  async view_entities_page_2(fx) {
    const { client, calls } = stub([fx]);
    const page = await client.viewEntities("ops_status", {
      after: "probe-a:2026-10-27",
      limit: 2,
    });
    expectRequest(calls[0], fx);
    expect(page.next).toBeNull();
    expect(page.revision).toBe(61);
  },

  async view_get(fx) {
    const { client, calls } = stub([fx]);
    const info = await client.getView("ops_status");
    expectRequest(calls[0], fx);
    expect(info.generation).toBe(2);
    expect(info.revision).toBe(62);
    expect(info.definition.ops).toHaveLength(5);
  },

  async view_latest(fx) {
    const { client, calls } = stub([fx]);
    const row = await client.latest("ops_status", "probe-a:2026-10-27");
    expectRequest(calls[0], fx);
    expect(row).not.toBeInstanceOf(NotModified);
    expect(row).not.toBeInstanceOf(ViewRowNotFound);
    const found = row as Exclude<typeof row, NotModified | ViewRowNotFound>;
    expect(found.state).toEqual({ ok: 1430, failed: 2 });
    expect(found.version).toBe(57);
    expect(found.etag).toBe(etagOf(fx));
  },

  async view_latest_not_found(fx) {
    const { client, calls } = stub([fx]);
    const row = await client.latest("ops_status", "probe-q:2026-10-27");
    expectRequest(calls[0], fx);
    expect(row).toBeInstanceOf(ViewRowNotFound);
    expect((row as ViewRowNotFound).ready).toBe(true);
  },

  async view_latest_not_modified(fx) {
    const { client, calls } = stub([fx]);
    const row = await client.latest("ops_status", "probe-a:2026-10-27", {
      ifNoneMatch: ifNoneMatchOf(fx),
    });
    expectRequest(calls[0], fx);
    expect(row).toBeInstanceOf(NotModified);
    expect((row as NotModified).etag).toBe(etagOf(fx));
  },

  async views_list(fx) {
    const { client, calls } = stub([fx]);
    const views = await client.listViews();
    expectRequest(calls[0], fx);
    expect(views.map((v) => [v.view, v.ready])).toEqual([
      ["ops_status", true],
      ["skill_catalog", false],
    ]);
  },
};

describe("contract fixtures", () => {
  test("fixture directory is not empty and names match file names", () => {
    expect(fixtures.size).toBeGreaterThan(0);
    for (const [file, fx] of fixtures) expect(fx.name).toBe(file);
  });

  test("every fixture has a runner and every runner a fixture", () => {
    expect([...fixtures.keys()].sort()).toEqual(Object.keys(runners).sort());
  });

  for (const [name, fx] of fixtures) {
    test(name, async () => {
      const runner = runners[name];
      if (!runner) throw new Error(`no test consumes fixture ${name}`);
      await runner(fx);
    });
  }
});

describe("contract scenarios", () => {
  test("latestMany reads every snapshot page", async () => {
    const page1 = fixture("view_entities_page_1");
    const page2 = fixture("view_entities_page_2");
    const { client, calls } = stub([page1, page2]);

    const keys: string[] = [];
    for await (const row of client.latestMany("ops_status", { limit: 2 })) keys.push(row.key);

    expectRequest(calls[0], page1);
    expectRequest(calls[1], page2);
    expect(keys).toEqual(["probe-a:2026-10-26", "probe-a:2026-10-27", "probe-z:2026-10-20"]);
  });

  test("sync snapshots, applies changes, then long-polls from the stored checkpoint", async () => {
    const served = [
      fixture("view_entities_page_1"),
      fixture("view_entities_page_2"),
      fixture("view_changes"),
      fixture("view_changes_empty_after_wait"),
    ];
    const controller = new AbortController();
    const { client, calls } = stub(served, (index) => {
      if (index === 3) controller.abort();
    });

    const state = new Map<string, { state: unknown; version: number }>();
    const checkpoints: unknown[] = [];
    for await (const item of client.sync("ops_status", {
      limit: 2,
      wait: 20,
      signal: controller.signal,
    })) {
      if (item.type === "upsert") state.set(item.key, { state: item.state, version: item.version });
      if (item.type === "delete") state.delete(item.key);
      if (item.type === "checkpoint") checkpoints.push(item.checkpoint);
    }

    expect(calls).toHaveLength(4);
    served.forEach((fx, i) => {
      expectRequest(calls[i], fx);
    });
    expect(Object.fromEntries(state)).toEqual({
      "probe-a:2026-10-26": { state: { ok: 1440, failed: 0 }, version: 40 },
      "probe-a:2026-10-27": { state: { ok: 1431, failed: 2 }, version: 62 },
    });
    expect(checkpoints).toEqual([
      { revision: 61, generation: 2 },
      { revision: 62, generation: 2 },
    ]);
  });

  test("sync re-reads the snapshot after view_reset", async () => {
    const served = [
      fixture("view_changes_reset"),
      fixture("view_entities_page_1"),
      fixture("view_entities_page_2"),
    ];
    const { client, calls } = stub(served);

    const items: ViewSyncItem[] = [];
    for await (const item of client.sync("ops_status", {
      checkpoint: { revision: 12, generation: 2 },
      limit: 2,
    })) {
      items.push(item);
      if (item.type === "checkpoint") break;
    }

    served.forEach((fx, i) => {
      expectRequest(calls[i], fx);
    });
    expect(items[0]).toEqual({ type: "reset", generation: 3 });
    expect(items.filter((i) => i.type === "upsert")).toHaveLength(3);
    expect(items[items.length - 1]).toEqual({
      type: "checkpoint",
      checkpoint: { revision: 61, generation: 2 },
    });
  });
});
