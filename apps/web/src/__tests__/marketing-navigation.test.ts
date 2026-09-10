import { describe, expect, it } from "vitest";
import { platformNavigationGroups, primaryNavigation, siteConfig } from "@/lib/config";

describe("marketing navigation", () => {
  it("keeps four top-level informational choices", () => {
    expect(primaryNavigation).toEqual([
      { href: "/use-cases", label: "Use cases" },
      { href: "/docs", label: "Docs" },
      { href: "/pricing", label: "Pricing" },
    ]);
    expect(["Platform", ...primaryNavigation.map(({ label }) => label)]).toHaveLength(4);
  });

  it("groups platform detail behind one progressive disclosure", () => {
    expect(platformNavigationGroups.map(({ label }) => label)).toEqual(["Core", "Build with it"]);
    expect(platformNavigationGroups.flatMap(({ items }) => items.map(({ href }) => href))).toEqual([
      "/what-is-allsource",
      "/platform/event-sourcing",
      "/platform/query-service",
      "/prime",
      "/event-sourcing/patterns",
      "/examples",
    ]);
  });

  it("moves campaign navigation to contextual surfaces", () => {
    const primaryHrefs: string[] = primaryNavigation.map(({ href }) => href);
    expect(primaryHrefs).not.toContain("/design-partners");

    const company = siteConfig.footer.find(({ title }) => title === "Company");
    expect(company?.links).toContainEqual({
      href: "/design-partners",
      text: "Design partners",
      icon: null,
    });
  });
});
