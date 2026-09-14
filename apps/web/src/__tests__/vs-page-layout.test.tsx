import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import MarketingLayout from "@/app/(marketing)/layout";
import { VsPageBody } from "@/app/(marketing)/vs/_components/VsPageBody";
import { competitors } from "@/app/(marketing)/vs/_data/competitors";

vi.mock("next/headers", () => ({
  cookies: async () => ({ get: () => undefined }),
}));

describe("comparison page layout", () => {
  it("uses the marketing shell exactly once", async () => {
    const layout = await MarketingLayout({
      children: <VsPageBody competitor={competitors.mem0} />,
    });

    render(layout);

    expect(screen.getAllByRole("banner")).toHaveLength(1);
    expect(screen.getAllByRole("main")).toHaveLength(1);
    expect(screen.getAllByRole("contentinfo")).toHaveLength(1);
  });
});
