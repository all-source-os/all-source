import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanAnalyticsUrl, initProductAnalytics, isAnalyticsTest } from "@/lib/product-analytics";

const posthogInit = vi.hoisted(() => vi.fn());
vi.mock("posthog-js", () => ({ posthog: { init: posthogInit } }));

describe("exception tracking", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
    window.__allsourceAnalytics = undefined;
  });

  it("sends only the exception type, never the message or stack", async () => {
    vi.stubEnv("NODE_ENV", "production");
    vi.stubEnv("NEXT_PUBLIC_POSTHOG_KEY", "phc_test");
    vi.stubEnv("NEXT_PUBLIC_POSTHOG_HOST", "https://ph.example/");
    const fetchMock = vi.fn().mockResolvedValue(new Response(null));
    vi.stubGlobal("fetch", fetchMock);

    await initProductAnalytics();
    window.dispatchEvent(
      new ErrorEvent("error", { message: "secret token abc", error: new Error("secret") })
    );

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe("https://ph.example/i/v0/e/");
    expect(init.credentials).toBe("omit");
    expect(init.body).not.toContain("secret");
    const { properties } = JSON.parse(init.body);
    expect(properties.$exception_type).toBe("ClientError");
    expect(properties.$exception_mechanism).toBe("window.error");
    expect(properties.exception_contract).toBe("type-only");

    const { before_send } = posthogInit.mock.calls[0]?.[1] ?? {};
    const event = before_send({
      event: "$exception",
      properties: { $exception_list: [{ value: "secret" }], $exception_message: "secret" },
    });
    expect(JSON.stringify(event)).not.toContain("secret");
    expect(event.properties.$exception_type).toBe("ClientError");
  });
});

describe("analytics traffic classification", () => {
  it("keeps ordinary canonical-host traffic in production", () => {
    expect(isAnalyticsTest("www.all-source.xyz", "")).toBe(false);
    expect(isAnalyticsTest("all-source.xyz", "?analytics_test=0")).toBe(false);
  });

  it("excludes explicitly tagged production QA", () => {
    expect(isAnalyticsTest("www.all-source.xyz", "?analytics_test=1")).toBe(true);
    expect(isAnalyticsTest("all-source.xyz", "?analytics_test=true")).toBe(true);
    expect(isAnalyticsTest("all-source.xyz", "?analytics_test=0&analytics_test=1")).toBe(true);
  });

  it("preserves a test session after its query disappears", () => {
    expect(isAnalyticsTest("www.all-source.xyz", "", true)).toBe(true);
    expect(isAnalyticsTest("www.all-source.xyz", "?analytics_test=0", true)).toBe(true);
  });

  it("never classifies preview hosts or lookalike domains as production", () => {
    expect(isAnalyticsTest("localhost", "")).toBe(true);
    expect(isAnalyticsTest("allsource-web.fly.dev", "")).toBe(true);
    expect(isAnalyticsTest("all-source.xyz.example.com", "")).toBe(true);
  });
});

describe("cleanAnalyticsUrl", () => {
  it("drops query strings and fragments", () => {
    expect(cleanAnalyticsUrl("https://www.all-source.xyz/docs?q=secret#section")).toBe(
      "https://www.all-source.xyz/docs"
    );
  });

  it("resolves paths against a trusted origin", () => {
    expect(cleanAnalyticsUrl("/pricing?campaign=private", "https://www.all-source.xyz")).toBe(
      "https://www.all-source.xyz/pricing"
    );
  });

  it("rejects invalid values", () => {
    expect(cleanAnalyticsUrl(null)).toBeUndefined();
    expect(cleanAnalyticsUrl("not a URL")).toBeUndefined();
  });
});
