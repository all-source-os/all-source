import type { MetadataRoute } from "next";
import { getBlogPosts } from "@/lib/blog";
import { siteConfig } from "@/lib/config";
import { integrations } from "@/lib/integrations";

const STATIC_PATHS = [
  "",
  "/about",
  "/architecture",
  "/agent-memory-restart-proof",
  "/blog",
  "/changelog",
  "/compare/agent-memory",
  "/compare/eventstoredb",
  "/connect",
  "/design-partners",
  "/docs",
  "/docs/api",
  "/docs/chronis",
  "/docs/mcp",
  "/docs/prime",
  "/docs/prime/concepts",
  "/docs/prime/embedded",
  "/docs/prime/http",
  "/docs/prime/mcp",
  "/docs/prime/quickstart",
  "/docs/tenant-setup",
  "/ecosystem",
  "/event-replay-debugging",
  "/event-sourcing-for-ai-agents",
  "/examples",
  "/install",
  "/platform/event-sourcing",
  "/platform/prime",
  "/platform/projections",
  "/platform/query-service",
  "/platform/stream-processing",
  "/pricing",
  "/prime",
  "/privacy",
  "/sdks",
  "/solutions/agent-memory",
  "/solutions/audit-compliance",
  "/solutions/financial-services",
  "/solutions/iot-telemetry",
  "/solutions/multi-tenant-saas",
  "/solutions/quant-intelligence",
  "/solutions/real-time-analytics",
  "/status",
  "/terms",
  "/use-cases",
  "/vs/letta",
  "/vs/mem0",
  "/vs/stoolap",
  "/vs/zep",
  "/what-is-allsource",
  "/what-is-an-event-store",
] as const;

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const posts = await getBlogPosts();

  const staticPages = STATIC_PATHS.map((path) => ({
    url: `${siteConfig.url}${path}`,
  }));

  const installPages = integrations.map((integration) => ({
    url: `${siteConfig.url}/install/${integration.slug}`,
  }));

  const blogPages = posts.map((post) => ({
    url: `${siteConfig.url}/blog/${post.slug}`,
    lastModified: new Date(post.updatedAt || post.publishedAt),
  }));

  return [...staticPages, ...installPages, ...blogPages];
}
