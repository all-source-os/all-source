import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { metadata } from "@/app/(marketing)/what-is-an-event-store/page";
import { siteConfig } from "@/lib/config";

const WEB_ROOT = path.resolve(__dirname, "../..");

function source(relativePath: string): string {
  return readFileSync(path.join(WEB_ROOT, relativePath), "utf-8");
}

describe("event store definition route", () => {
  it("targets one canonical event-store database definition", () => {
    expect(metadata.title).toBe("What Is an Event Store Database? Uses and Trade-offs");
    expect(metadata.alternates).toMatchObject({ canonical: "/what-is-an-event-store" });
    expect(metadata.openGraph).toMatchObject({
      url: `${siteConfig.url}/what-is-an-event-store`,
    });

    const page = source("src/app/(marketing)/what-is-an-event-store/page.tsx");
    expect(page).toContain("Event sourcing is the model; the event store is the database");
    expect(page).toMatch(/trace results back to source\s+events/);
    expect(page).toContain("reconstruct earlier state");
    expect(page).toContain("Differences between an event store database");
    expect(page).toContain('"@type": "TechArticle"');
  });

  it("connects homepage, docs, footer, and sitemap with rendered links", () => {
    expect(source("src/components/sections/hero.tsx")).toContain('href="/what-is-an-event-store"');
    expect(source("src/app/(marketing)/docs/page.tsx")).toContain(
      'href: "/what-is-an-event-store"'
    );
    expect(source("src/app/sitemap.ts")).toContain('"/what-is-an-event-store"');

    const platform = siteConfig.footer.find(({ title }) => title === "Platform");
    expect(platform?.links).toContainEqual({
      href: "/what-is-an-event-store",
      text: "What is an event store?",
      icon: null,
    });
  });

  it("uses a fixed privacy-safe CTA allowlist", () => {
    const link = source("src/components/tracked-marketing-link.tsx");
    expect(link).toContain('"api_docs" | "live_demo" | "signup"');
    expect(link).toContain('trackProductEvent("marketing_cta_clicked"');
    expect(link).not.toContain("searchParams");
    expect(link).not.toContain("document.referrer");
  });
});
