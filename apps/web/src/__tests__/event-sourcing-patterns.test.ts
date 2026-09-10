import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  eventSourcingPatterns,
  getEventSourcingPattern,
  getPatternWordCount,
} from "@/lib/event-sourcing-patterns";
import {
  eventSourcingPatternListSchema,
  softwareApplicationSchema,
  techArticleSchema,
} from "@/lib/structured-data";

const WEB_ROOT = path.resolve(__dirname, "../..");

function source(relativePath: string): string {
  return readFileSync(path.join(WEB_ROOT, relativePath), "utf-8");
}

describe("event-store category and event-sourcing pSEO cluster", () => {
  it("publishes exactly ten bounded, unique production patterns", () => {
    expect(eventSourcingPatterns).toHaveLength(10);

    const unique = (values: readonly string[]) => new Set(values).size;
    expect(unique(eventSourcingPatterns.map(({ slug }) => slug))).toBe(10);
    expect(unique(eventSourcingPatterns.map(({ title }) => title))).toBe(10);
    expect(unique(eventSourcingPatterns.map(({ description }) => description))).toBe(10);
    expect(unique(eventSourcingPatterns.map(({ directAnswer }) => directAnswer))).toBe(10);
  });

  it("keeps every pattern substantial, indexable, and internally connected", () => {
    for (const pattern of eventSourcingPatterns) {
      expect(pattern.title.length, `${pattern.slug} title length`).toBeGreaterThanOrEqual(30);
      expect(pattern.title.length, `${pattern.slug} title length`).toBeLessThanOrEqual(60);
      expect(
        pattern.description.length,
        `${pattern.slug} description length`
      ).toBeGreaterThanOrEqual(120);
      expect(pattern.description.length, `${pattern.slug} description length`).toBeLessThanOrEqual(
        160
      );
      expect(getPatternWordCount(pattern), `${pattern.slug} word count`).toBeGreaterThanOrEqual(
        300
      );
      expect(pattern.decisions).toHaveLength(3);
      expect(pattern.failureModes).toHaveLength(3);
      expect(pattern.checklist.length).toBeGreaterThanOrEqual(5);
      expect(pattern.related).toHaveLength(3);
      expect(new Set(pattern.related).size).toBe(3);
      expect(pattern.related).not.toContain(pattern.slug);

      for (const relatedSlug of pattern.related) {
        expect(
          getEventSourcingPattern(relatedSlug),
          `${pattern.slug} -> ${relatedSlug}`
        ).toBeDefined();
      }
    }
  });

  it("statically renders each self-canonical page with article and breadcrumb schema", () => {
    const route = source("src/app/(marketing)/event-sourcing/patterns/[slug]/page.tsx");

    expect(route).toContain("export const dynamicParams = false");
    expect(route).toContain("generateStaticParams");
    expect(route).toContain("canonical:");
    expect(route).toContain("/event-sourcing/patterns/");
    expect(route).toContain("pattern.slug");
    expect(route).toContain("breadcrumbSchema");
    expect(route).toContain("techArticleSchema");

    const pattern = eventSourcingPatterns[0];
    expect(pattern).toBeDefined();
    if (!pattern) throw new Error("Expected at least one event-sourcing pattern");
    const article = techArticleSchema(pattern, getPatternWordCount(pattern));
    expect(article["@type"]).toBe("TechArticle");
    expect(article.url).toBe(`https://www.all-source.xyz/event-sourcing/patterns/${pattern.slug}`);
    expect(article.about).toContainEqual({ "@type": "Thing", name: "Event store database" });
  });

  it("adds hub and all pattern routes to sitemap and site-wide navigation", () => {
    const sitemap = source("src/app/sitemap.ts");
    const config = source("src/lib/config.ts");

    expect(sitemap).toContain('"/event-sourcing/patterns"');
    expect(sitemap).toContain("eventSourcingPatterns.map");
    expect(config.match(/href: "\/event-sourcing\/patterns"/g)?.length).toBeGreaterThanOrEqual(2);

    const itemList = eventSourcingPatternListSchema(eventSourcingPatterns);
    expect(itemList.numberOfItems).toBe(10);
    expect(itemList.itemListElement).toHaveLength(10);
  });

  it("uses event-store database as primary category and Prime as derived layer", () => {
    const homepage = source("src/components/sections/hero.tsx");
    const rootLayout = source("src/app/layout.tsx");
    const productIdentity = source("src/lib/product-verticals.ts");
    const software = softwareApplicationSchema();

    expect(homepage).toContain("Event store database built for event sourcing.");
    expect(homepage).toContain("through Prime");
    expect(rootLayout).toContain("Event Store Database for Event Sourcing | AllSource");
    expect(productIdentity).toContain("purpose-built event store database for event sourcing");
    expect(software.applicationSubCategory).toBe("Event store database");
  });
});
