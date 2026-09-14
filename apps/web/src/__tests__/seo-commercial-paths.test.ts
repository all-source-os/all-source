import { readFileSync } from "node:fs";
import path from "node:path";
import { render, screen, within } from "@testing-library/react";
import { createElement } from "react";
import { describe, expect, it } from "vitest";
import { metadata as agentMemoryCompare } from "@/app/(marketing)/compare/agent-memory/layout";
import CompareAgentMemoryPage from "@/app/(marketing)/compare/agent-memory/page";
import { metadata as examples } from "@/app/(marketing)/examples/page";
import { metadata as platformPrime } from "@/app/(marketing)/platform/prime/layout";
import { metadata as queryService } from "@/app/(marketing)/platform/query-service/page";
import { metadata as prime } from "@/app/(marketing)/prime/layout";
import { metadata as multiTenant } from "@/app/(marketing)/solutions/multi-tenant-saas/layout";
import { siteConfig } from "@/lib/config";

const WEB_ROOT = path.resolve(__dirname, "../..");

function source(relativePath: string): string {
  return readFileSync(path.join(WEB_ROOT, relativePath), "utf-8");
}

const commercialRoutes = [
  ["/compare/agent-memory", agentMemoryCompare],
  ["/examples", examples],
  ["/platform/prime", platformPrime],
  ["/platform/query-service", queryService],
  ["/prime", prime],
  ["/solutions/multi-tenant-saas", multiTenant],
] as const;

describe("buyer-near SEO paths", () => {
  it("gives each route a distinct, concise title, description, and self-canonical URL", () => {
    const titles = new Set<string>();

    for (const [route, metadata] of commercialRoutes) {
      const title = metadata.title;
      const description = metadata.description;

      expect(typeof title, `${route} title type`).toBe("string");
      expect(typeof description, `${route} description type`).toBe("string");
      if (typeof title !== "string" || typeof description !== "string") continue;

      expect(title.length, `${route} title length`).toBeGreaterThanOrEqual(30);
      expect(title.length, `${route} title length`).toBeLessThanOrEqual(60);
      expect(description.length, `${route} description length`).toBeGreaterThanOrEqual(120);
      expect(description.length, `${route} description length`).toBeLessThanOrEqual(160);
      expect(metadata.alternates).toMatchObject({ canonical: route });
      expect(metadata.openGraph).toMatchObject({ url: `${siteConfig.url}${route}` });
      titles.add(title);
    }

    expect(titles.size).toBe(commercialRoutes.length);
  });

  it("connects comparison and field reports to the restart proof and solution hub", () => {
    for (const article of [
      "content/reconstructing-agent-memory-in-rust.mdx",
      "content/why-your-agents-memory-returned-nothing.mdx",
    ]) {
      const body = source(article);
      expect(body, `${article} proof link`).toContain("/agent-memory-restart-proof");
      expect(body, `${article} solution link`).toContain("/solutions/agent-memory");
    }

    const comparison = source("src/app/(marketing)/compare/agent-memory/page.tsx");
    expect(comparison).toContain("/agent-memory-restart-proof");
    expect(comparison).toContain("/solutions/agent-memory");
    expect(comparison).toContain("<table");
    expect(comparison).toContain("decisionMatrix.map");
    expect(comparison).toContain(
      "learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing"
    );
    expect(comparison).toContain("help.openai.com/en/articles/8590148");
  });

  it("renders a five-row cited decision table and reachable proof link", () => {
    render(createElement(CompareAgentMemoryPage));

    const table = screen.getByRole("table", {
      name: "Agent memory approaches, best fits, and limitations",
    });
    expect(within(table).getAllByRole("row")).toHaveLength(6);
    expect(within(table).getAllByRole("link")).toHaveLength(5);
    expect(screen.getByRole("link", { name: "Run the restart proof" })).toHaveAttribute(
      "href",
      "/agent-memory-restart-proof"
    );
  });

  it("keeps visible restart steps without deprecated HowTo structured data", () => {
    const proof = source("src/app/(marketing)/agent-memory-restart-proof/page.tsx");
    expect(proof).toContain("Run restart proof");
    expect(proof).toContain("breadcrumbSchema");
    expect(proof).not.toContain('"@type": "HowTo"');
  });
});
