import { describe, expect, it } from "vitest";
import {
  type CatalogTier,
  PriceUnavailable,
  pricingSignupHref,
  resolveBilledPrice,
  resolveYearlyPerMonth,
} from "@/lib/pricing-catalog";

const indie: CatalogTier = {
  tier: "indie",
  monthly: { cents: 1899, formatted: "£18.99" },
  annual: { cents: 18199, formatted: "£181.99", per_month: "£15.17" },
};

describe("billing price presentation", () => {
  it("uses charged annual total, not the monthly equivalent, as yearly headline", () => {
    expect(resolveBilledPrice(indie, "£20", "monthly")).toBe("£18.99");
    expect(resolveBilledPrice(indie, "£20", "annual")).toBe("£181.99");
    expect(resolveYearlyPerMonth(indie, "£20")).toBe("£15.17");
  });

  it("does not invent a paid price when catalog is missing", () => {
    expect(resolveBilledPrice(undefined, "£20", "monthly")).toBe(PriceUnavailable);
    expect(resolveBilledPrice(undefined, "£20", "annual")).toBe(PriceUnavailable);
    expect(resolveBilledPrice(undefined, "Custom", "annual")).toBe("Custom");
  });

  it("carries tier and period from pricing to billing", () => {
    const href = pricingSignupHref("studio", "annual");
    const url = new URL(href, "https://www.all-source.xyz");
    expect(url.pathname).toBe("/signup");
    expect(url.searchParams.get("next")).toBe("/dashboard/billing?plan=studio&period=annual");
  });
});
