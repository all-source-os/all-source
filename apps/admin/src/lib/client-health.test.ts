/**
 * Run with: `bun test src/lib/client-health.test.ts` (from apps/admin).
 */

/// <reference types="bun" />
import { afterEach, describe, expect, test } from "bun:test";
import {
  countTenants,
  formatServedPercent,
  groupReleases,
  orderTenants,
  shareTone,
  verdictFor,
} from "./client-health";
import {
  type ClientHealthNow,
  fetchClientHealth,
  fetchMetricsSummary,
  parseClientHealth,
  type TenantHealth,
} from "./metrics-api";

function tenant(
  name: string,
  state: TenantHealth["state"],
  served = 0,
  baseline = 0
): TenantHealth {
  return {
    tenant: name,
    served_5m: served,
    refused_5m: 0,
    failed_5m: 0,
    baseline_per_min: baseline,
    state,
  };
}

const now = (over: Partial<ClientHealthNow>): ClientHealthNow => ({
  window_seconds: 300,
  served: 0,
  refused_auth: 0,
  throttled: 0,
  failed: 0,
  total: 0,
  served_ratio: null,
  ...over,
});

describe("verdictFor", () => {
  test("healthy traffic reads as served, in the ok tone", () => {
    const v = verdictFor(
      now({ served: 1409, throttled: 3, total: 1412, served_ratio: 1409 / 1412 })
    );
    expect(v.tone).toBe("ok");
    expect(v.text).toBe("Clients served: 99.7% of 1,412 requests in the last 5 min.");
  });

  test("an auth outage reads as refused (401/403), in the bad tone", () => {
    const v = verdictFor(now({ refused_auth: 821, total: 821, served_ratio: 0 }));
    expect(v.tone).toBe("bad");
    expect(v.text).toBe(
      "Clients refused: 821 of 821 requests refused (401/403) in the last 5 min."
    );
  });

  test("between 80% and 95% served is a warning naming the dominant refusal", () => {
    const v = verdictFor(now({ served: 900, throttled: 100, total: 1000, served_ratio: 0.9 }));
    expect(v.tone).toBe("warn");
    expect(v.text).toContain("100 of 1,000 requests refused (429)");
  });

  test("exactly 95% is ok, just under 80% is bad", () => {
    expect(verdictFor(now({ served: 95, failed: 5, total: 100, served_ratio: 0.95 })).tone).toBe(
      "ok"
    );
    expect(verdictFor(now({ served: 79, failed: 21, total: 100, served_ratio: 0.79 })).tone).toBe(
      "bad"
    );
  });

  test("no data is unknown, never a healthy zero", () => {
    expect(verdictFor(null).tone).toBe("unknown");
    const empty = verdictFor(now({}));
    expect(empty.tone).toBe("unknown");
    expect(empty.text).toBe("No client data requests in the last 5 min.");
  });

  test("zero requests while tenants normally send traffic is bad, not neutral", () => {
    const v = verdictFor(now({}), [
      tenant("onboard", "quiet", 0, 1170.4),
      tenant("anonymous", "idle", 0, 500),
    ]);
    expect(v.tone).toBe("bad");
    expect(v.text).toBe(
      "No client requests reached the control plane in the last 5 min (expected ~1,170/min)."
    );
  });

  test("zero requests from tenants that barely send traffic stays unknown", () => {
    expect(verdictFor(now({}), [tenant("small", "idle", 0, 2)]).tone).toBe("unknown");
  });

  test("5xx-dominated traffic reads as failing, not refused", () => {
    const v = verdictFor(now({ served: 10, failed: 90, total: 100, served_ratio: 0.1 }));
    expect(v.tone).toBe("bad");
    expect(v.text).toBe("Clients failing: 90 of 100 requests failed (5xx) in the last 5 min.");
  });
});

describe("formatServedPercent", () => {
  test("floors so a refused request never rounds up to 100.0%", () => {
    expect(formatServedPercent(0.9996)).toBe("99.9%");
    expect(formatServedPercent(1)).toBe("100.0%");
    expect(formatServedPercent(0)).toBe("0.0%");
  });
});

describe("shareTone", () => {
  test("grades a share of the total", () => {
    expect(shareTone(0, 1000, 0.01, 0.2)).toBe("ok");
    expect(shareTone(50, 1000, 0.01, 0.2)).toBe("warn");
    expect(shareTone(500, 1000, 0.01, 0.2)).toBe("bad");
  });
});

describe("orderTenants", () => {
  test("quiet then refused rows come first, then served by volume, idle last", () => {
    const ordered = orderTenants([
      tenant("idle-co", "idle"),
      tenant("small", "served", 10),
      tenant("anonymous", "refused"),
      tenant("big", "served", 17_000),
      tenant("onboard", "quiet", 2),
    ]);
    expect(ordered.map((t) => t.tenant)).toEqual([
      "onboard",
      "anonymous",
      "big",
      "small",
      "idle-co",
    ]);
  });

  test("does not mutate its input", () => {
    const input = [tenant("b", "served", 1), tenant("a", "quiet")];
    orderTenants(input);
    expect(input.map((t) => t.tenant)).toEqual(["b", "a"]);
  });
});

describe("countTenants", () => {
  test("counts quiet and refused separately and leaves out anonymous", () => {
    expect(
      countTenants([
        tenant("a", "served", 5),
        tenant("b", "quiet"),
        tenant("c", "refused"),
        tenant("anonymous", "refused"),
      ])
    ).toEqual({ served: 1, quiet: 1, refused: 1 });
  });
});

describe("groupReleases", () => {
  test("machine starts within one step share one marker", () => {
    const g = groupReleases(
      [
        { app: "allsource-control-plane", instance: "a", started_at: "2026-10-26T11:46:00Z" },
        { app: "allsource-core", instance: "c", started_at: "2026-10-26T11:46:40Z" },
        { app: "allsource-control-plane", instance: "b", started_at: "2026-10-26T11:46:20Z" },
        { app: "allsource-core", instance: "c", started_at: "2026-10-26T13:00:00Z" },
      ],
      60_000
    );
    expect(g.map((x) => x.label)).toEqual(["control-plane ×2, core", "core"]);
    expect(g[0]?.ts).toBe(Date.parse("2026-10-26T11:46:00Z"));
  });
});

describe("parseClientHealth", () => {
  test("missing arrays stay empty instead of becoming zeros", () => {
    const r = parseClientHealth({ configured: true, range: "1h" });
    expect(r.series).toEqual([]);
    expect(r.tenants).toEqual([]);
    expect(r.now).toBeNull();
  });

  test("a now block missing a count is not measured, not partly zero", () => {
    const r = parseClientHealth({ configured: true, range: "1h", now: { total: 0, served: 0 } });
    expect(r.now).toBeNull();
  });

  test("a complete now block keeps a null ratio null", () => {
    const r = parseClientHealth({
      configured: true,
      range: "1h",
      now: {
        window_seconds: 300,
        served: 0,
        refused_auth: 0,
        throttled: 0,
        failed: 0,
        total: 0,
        served_ratio: null,
      },
    });
    expect(r.now?.total).toBe(0);
    expect(r.now?.served_ratio).toBeNull();
  });
});

const realFetch = globalThis.fetch;
afterEach(() => {
  globalThis.fetch = realFetch;
});

function mockFetch(status: number, body: unknown) {
  globalThis.fetch = (async () =>
    new Response(JSON.stringify(body), {
      status,
      headers: { "Content-Type": "application/json" },
    })) as unknown as typeof fetch;
}

describe("fetchClientHealth", () => {
  test("configured=false is the not-configured state", async () => {
    mockFetch(200, { configured: false, range: "1h", generated_at: "2026-10-26T12:00:00Z" });
    expect(await fetchClientHealth("1h")).toEqual({ kind: "not_configured" });
  });

  test("a 502 carries the upstream error text", async () => {
    mockFetch(502, { configured: true, error: "requests now: prometheus: HTTP 500: boom" });
    const r = await fetchClientHealth("1h");
    expect(r.kind).toBe("error");
    if (r.kind === "error") expect(r.message).toContain("HTTP 502");
    if (r.kind === "error") expect(r.message).toContain("boom");
  });

  test("an expired admin session is a sign-in error, not a Prometheus error", async () => {
    mockFetch(401, { error: "unauthorized" });
    const r = await fetchClientHealth("1h");
    expect(r.kind).toBe("error");
    if (r.kind === "error") {
      expect(r.message).toContain("signing in again");
      expect(r.message).not.toContain("Prometheus");
    }
  });

  test("a network failure is an error, not an empty report", async () => {
    globalThis.fetch = (async () => {
      throw new TypeError("fetch failed");
    }) as unknown as typeof fetch;
    const r = await fetchClientHealth("24h");
    expect(r.kind).toBe("error");
  });

  test("a configured report is ok", async () => {
    mockFetch(200, {
      configured: true,
      range: "1h",
      step_seconds: 60,
      generated_at: "2026-10-26T12:00:00Z",
      now: {
        window_seconds: 300,
        served: 10,
        refused_auth: 0,
        throttled: 0,
        failed: 0,
        total: 10,
        served_ratio: 1,
      },
      series: [{ t: "2026-10-26T11:59:00Z", served: 10, refused_auth: 0, throttled: 0, failed: 0 }],
    });
    const r = await fetchClientHealth("1h");
    expect(r.kind).toBe("ok");
    if (r.kind === "ok") expect(r.report.series).toHaveLength(1);
  });
});

describe("fetchMetricsSummary", () => {
  test("a missing or zero Core series is not measured", async () => {
    mockFetch(200, { events_total: 0, uptime_seconds: 0 });
    expect(await fetchMetricsSummary()).toEqual({ events_total: null, query_latency_p99_ms: null });
  });

  test("a real value passes through", async () => {
    mockFetch(200, { events_total: 1_420_000, query_latency_p99_ms: 11.9 });
    expect(await fetchMetricsSummary()).toEqual({
      events_total: 1_420_000,
      query_latency_p99_ms: 11.9,
    });
  });
});
