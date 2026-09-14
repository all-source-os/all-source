import { fireEvent, render, screen } from "@testing-library/react";
import { SWRConfig } from "swr";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PlanCards } from "@/components/billing/plan-cards";
import PricingSection from "@/components/sections/pricing";
import type { Catalog } from "@/lib/pricing-catalog";

const catalog: Catalog = {
  currency: "GBP",
  tiers: [
    {
      tier: "indie",
      monthly: { cents: 1899, formatted: "£18.99" },
      annual: { cents: 18199, formatted: "£181.99", per_month: "£15.17" },
    },
    {
      tier: "studio",
      monthly: { cents: 7899, formatted: "£78.99" },
      annual: { cents: 75799, formatted: "£757.99", per_month: "£63.17" },
    },
    {
      tier: "scale",
      monthly: { cents: 29899, formatted: "£298.99" },
      annual: { cents: 286999, formatted: "£2869.99", per_month: "£239.17" },
    },
  ],
};

afterEach(() => vi.unstubAllGlobals());

describe("pricing section", () => {
  it("loads current prices after a static homepage render", async () => {
    const fetchCatalog = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => catalog,
    });
    vi.stubGlobal("fetch", fetchCatalog);

    render(
      <SWRConfig value={{ provider: () => new Map() }}>
        <PricingSection />
      </SWRConfig>
    );

    expect(await screen.findByText("£18.99")).toBeInTheDocument();
    expect(fetchCatalog).toHaveBeenCalledWith("/api/billing/catalog");
  });

  it("shows annual charges as primary prices and carries annual selection", () => {
    render(<PricingSection catalog={catalog} />);
    fireEvent.click(screen.getByRole("button", { name: "Yearly" }));

    expect(screen.getByText("£181.99")).toBeInTheDocument();
    expect(screen.getByText("£757.99")).toBeInTheDocument();
    expect(screen.getByText("£2869.99")).toBeInTheDocument();
    expect(screen.getByText("£15.17/mo equivalent · charged annually")).toBeInTheDocument();
    expect(screen.queryByText("-20%")).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Start Indie" })).toHaveAttribute(
      "href",
      "/signup?next=%2Fdashboard%2Fbilling%3Fplan%3Dindie%26period%3Dannual"
    );
  });

  it("shows selected annual plan and charge on the billing screen", () => {
    render(<PlanCards catalog={catalog} currentPlan="free" isYearly selectedTier="studio" />);
    expect(screen.getByText("£757.99")).toBeInTheDocument();
    expect(screen.getByText("£63.17/mo equivalent · charged annually")).toBeInTheDocument();
    expect(screen.getByText("Selected from pricing")).toBeInTheDocument();
  });
});
