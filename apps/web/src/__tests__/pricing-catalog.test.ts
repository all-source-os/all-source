import { describe, expect, it } from "vitest";
import {
  type CatalogTier,
  minimumAnnualSavingsPercent,
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

  it("derives conservative yearly savings from all provider price pairs", () => {
    const catalog = {
      currency: "GBP",
      tiers: [
        indie,
        {
          tier: "studio",
          monthly: { cents: 7899, formatted: "£78.99" },
          annual: { cents: 75799, formatted: "£757.99" },
        },
        {
          tier: "scale",
          monthly: { cents: 29899, formatted: "£298.99" },
          annual: { cents: 286999, formatted: "£2869.99" },
        },
      ],
    };
    const tiers = ["indie", "studio", "scale"];

    expect(minimumAnnualSavingsPercent(catalog, tiers)).toBe(20);
    expect(minimumAnnualSavingsPercent({ ...catalog, stale: true }, tiers)).toBeNull();
    expect(minimumAnnualSavingsPercent({ ...catalog, tiers: [indie] }, tiers)).toBeNull();
    expect(minimumAnnualSavingsPercent(null, tiers)).toBeNull();
    expect(
      minimumAnnualSavingsPercent(
        {
          ...catalog,
          tiers: [
            { ...indie, annual: { cents: 20500, formatted: "£205" } },
            ...catalog.tiers.slice(1),
          ],
        },
        tiers
      )
    ).toBe(10);
  });
});
