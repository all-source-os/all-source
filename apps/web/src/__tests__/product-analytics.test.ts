import { describe, expect, it } from "vitest";
import { cleanAnalyticsUrl } from "@/lib/product-analytics";

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
