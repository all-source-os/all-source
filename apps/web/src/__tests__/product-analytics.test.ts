import { describe, expect, it } from "vitest";
import { cleanAnalyticsUrl, isAnalyticsTest } from "@/lib/product-analytics";

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
