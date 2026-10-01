import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import sitemap from "@/app/sitemap";
import { getPost } from "@/lib/blog";
import { siteConfig } from "@/lib/config";
import { eventSourcingPatterns } from "@/lib/event-sourcing-patterns";

const root = path.resolve(__dirname, "../..");
const slugs = ["event-store-vs-database", "cqrs-vs-event-sourcing"];

describe("event store search content", () => {
  it.each(
    slugs
  )("renders %s with a comparison, sources and valid internal destinations", async (slug) => {
    const post = await getPost(slug);
    expect(post).not.toBeNull();
    if (!post) throw new Error(`Missing post ${slug}`);
    const document = new DOMParser().parseFromString(post.source, "text/html");
    expect(document.querySelectorAll("table").length).toBeGreaterThan(0);
    expect(document.querySelectorAll("h2").length).toBeGreaterThanOrEqual(5);
    expect(document.querySelector('a[href="/platform/event-sourcing"]')).not.toBeNull();
    expect(document.querySelector('a[href="/event-replay-validation-checklist"]')).not.toBeNull();
    expect(document.querySelector('a[href^="https://learn.microsoft.com/"]')).not.toBeNull();
    expect(post.metadata.summary.length).toBeLessThanOrEqual(160);
    expect(existsSync(path.join(root, "public", post.metadata.image))).toBe(true);
    expect(new URL(post.metadata.imageUrl).pathname).toMatch(/\.(png|jpe?g|webp)$/i);

    const routes = new Set((await sitemap()).map((entry) => new URL(entry.url).pathname));
    for (const link of document.querySelectorAll<HTMLAnchorElement>('a[href^="/"]')) {
      expect(routes.has(link.pathname), `${slug}: ${link.pathname}`).toBe(true);
    }
  });

  it("includes the new guide in the sitemap and links from both product and pattern hub", async () => {
    const route = "/blog/cqrs-vs-event-sourcing";
    expect(
      (await sitemap()).filter((entry) => entry.url === `${siteConfig.url}${route}`)
    ).toHaveLength(1);
    for (const file of [
      "src/app/(marketing)/platform/event-sourcing/page.tsx",
      "src/app/(marketing)/event-sourcing/patterns/page.tsx",
    ]) {
      expect(readFileSync(path.join(root, file), "utf8")).toContain(route);
    }
    expect(
      eventSourcingPatterns.some((pattern) => pattern.slug === "projections-read-models")
    ).toBe(true);
  });

  it("removes unsupported latency and storage promises from the database comparison", async () => {
    const post = await getPost("event-store-vs-database");
    expect(post?.source).toContain("event store is a database");
    for (const claim of ["&lt;100ms", "60-80%", "costs pennies", "reconstruct any past state"]) {
      expect(post?.source).not.toContain(claim);
    }
  });
});
