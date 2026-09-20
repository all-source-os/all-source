import Link from "next/link";
import { siteConfig } from "@/lib/config";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "Documentation",
  description: `Documentation for ${siteConfig.name} event storage, stream processing, MCP access, and Prime agent memory.`,
  canonical: "/docs",
});

const cards = [
  {
    title: "Event Store Fundamentals",
    description:
      "Understand immutable streams, replay, projections, provenance, temporal state, and the trade-offs of event sourcing.",
    href: "/what-is-an-event-store",
    external: false,
  },
  {
    title: "Getting Started",
    description: "Set up AllSource Core, ingest your first events, and query them in minutes.",
    href: "https://github.com/all-source-os/all-source#quick-start",
    external: true,
  },
  {
    title: "API Reference",
    description: "REST endpoints for events, projections, schemas, snapshots, and pipelines.",
    href: "/docs/api",
    external: false,
  },
  {
    title: "MCP Integration",
    description: "Connect AllSource to Claude Desktop with the Model Context Protocol server.",
    href: "/docs/mcp",
    external: false,
  },
  {
    title: "Prime — Agent Memory",
    description:
      "Knowledge graph + vector search + compressed index in one binary. Persistent cross-domain memory for AI agents.",
    href: "/docs/prime",
    external: false,
  },
  {
    title: "Chronis CLI",
    description:
      "Agent-native task CLI with TOON output (~50% fewer tokens), event sourcing, cascade actions, and archiving.",
    href: "/docs/chronis",
    external: false,
  },
  {
    title: "SDKs",
    description: "Client libraries for Rust, Go, Python, and TypeScript.",
    href: "https://github.com/all-source-os/all-source#sdks",
    external: true,
  },
  {
    title: "Architecture",
    description:
      "WAL-backed durability, Parquet columnar storage, and leader-follower replication.",
    href: "https://github.com/all-source-os/all-source/tree/main/docs",
    external: true,
  },
  {
    title: "GitHub Repository",
    description: "Source code, issues, discussions, and contribution guidelines.",
    href: "https://github.com/all-source-os/all-source",
    external: true,
  },
];

export default function DocsPage() {
  return (
    <div className="mx-auto w-full max-w-screen-md px-4 lg:px-8 py-24">
      <h1 className="text-3xl font-bold text-foreground sm:text-4xl mb-2">
        AllSource documentation
      </h1>
      <p className="text-lg text-muted-foreground mb-10">
        Everything you need to build with AllSource — from quickstart guides to full API reference.
      </p>

      <div className="grid gap-4 sm:grid-cols-2">
        {cards.map((card) => {
          const Component = card.external ? "a" : Link;
          const props = card.external
            ? {
                href: card.href,
                target: "_blank" as const,
                rel: "noopener noreferrer",
              }
            : { href: card.href };

          return (
            <Component
              key={card.title}
              {...props}
              className="group rounded-xl border border-border p-5 transition-all hover:border-primary/50 hover:bg-muted/50"
            >
              <h2 className="font-semibold text-foreground mb-1">
                {card.title}
                {card.external && (
                  <span className="ml-1 text-muted-foreground text-sm">&#8599;</span>
                )}
              </h2>
              <p className="text-sm text-muted-foreground leading-relaxed">{card.description}</p>
            </Component>
          );
        })}
      </div>
    </div>
  );
}
