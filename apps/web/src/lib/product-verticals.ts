export type ProductVerticalId = "core" | "query" | "prime" | "hosted" | "mcp";

export type ProductVertical = {
  id: ProductVerticalId;
  name: string;
  role: string;
  path: string;
  directAnswer: string;
  stores: string;
  useWhen: string;
  notThis: string;
};

/**
 * Canonical AllSource product taxonomy.
 *
 * Keep public product-map copy, JSON-LD, and GEO tests on this source instead
 * of restating boundaries independently across pages.
 */
export const productVerticals: readonly ProductVertical[] = [
  {
    id: "core",
    name: "AllSource Core",
    role: "Store",
    path: "/platform/event-sourcing",
    directAnswer:
      "Rust event-store database for ordered, immutable application events, replay, and point-in-time inspection.",
    stores:
      "Application events and system metadata in a CRC32-checked WAL, Parquet persistence, and concurrent indexes.",
    useWhen: "Your application needs durable history, provenance, replay, or event-sourced state.",
    notThis: "Not PostgreSQL, a vector database, a cache, or the hosted control plane.",
  },
  {
    id: "query",
    name: "AllSource Query Service",
    role: "Read",
    path: "/platform/query-service",
    directAnswer:
      "Stateless Elixir/Phoenix read plane that separates tenant-scoped HTTP queries, realtime channels, analytics endpoints, and rebuildable read models over Core events.",
    stores:
      "No durable source data; ETS caches and per-tenant read models are rebuilt from Core's event history.",
    useWhen:
      "Applications need different read shapes for request-response APIs, live interfaces, analytics, or current-state projections.",
    notThis: "Not another database and never the source of truth for accepted events.",
  },
  {
    id: "prime",
    name: "AllSource Prime",
    role: "Remember",
    path: "/prime",
    directAnswer:
      "Agent-memory engine that combines graph relationships, vector retrieval, temporal context, and source-event provenance.",
    stores:
      "Agent-memory events and derived graph/vector indexes, locally or in tenant-scoped Core persistence.",
    useWhen:
      "An AI agent must remember across sessions and trace a recalled fact to its source events.",
    notThis: "Not a pricing tier, a separate company, or the 55-tool event-store connector.",
  },
  {
    id: "hosted",
    name: "Hosted AllSource",
    role: "Operate",
    path: "/pricing",
    directAnswer:
      "Managed AllSource deployment with tenant provisioning, authentication, quotas, billing, and public API access.",
    stores:
      "Tenant events and operational metadata in Core; Query Service keeps only rebuildable caches and read models.",
    useWhen: "You want AllSource without operating the stateful services yourself.",
    notThis: "Not a permanent free tier and not a public MCP-over-HTTP endpoint.",
  },
  {
    id: "mcp",
    name: "AllSource MCP connectors",
    role: "Connect",
    path: "/docs/mcp",
    directAnswer:
      "Tool interfaces that let MCP-capable agents use either the event store or Prime from a local stdio process.",
    stores: "Nothing independently; connectors call Core, the hosted gateway, or Prime.",
    useWhen:
      "Claude, Cursor, Codex, or another MCP client needs explicit tools for events or memory.",
    notThis:
      "Not one combined tool registry: event-store MCP and Prime MCP are separate connectors.",
  },
] as const;

export const allsourceIdentity = {
  canonicalName: "AllSource Event Store",
  shortName: "AllSource",
  domain: "all-source.xyz",
  directAnswer:
    "AllSource is a purpose-built event store database for event sourcing. Core stores ordered, immutable application events; Query Service separates tenant-facing HTTP, realtime, analytics, and projection reads; Prime derives agent memory from same durable history; hosted services operate the stack; MCP connectors expose tools.",
  disambiguation:
    "It is the developer product at all-source.xyz and github.com/all-source-os/all-source. It is not Esri ArcGIS AllSource, the all-source intelligence discipline, or unrelated audience-data and logistics companies using a similar name.",
} as const;

export const productIdentityFaqs = [
  {
    question: "What is AllSource?",
    answer: allsourceIdentity.directAnswer,
  },
  {
    question: "What does AllSource Query Service do?",
    answer:
      "Query Service is the stateless read plane over Core. It serves tenant-scoped HTTP queries, Phoenix Channel realtime streams, analytics endpoints, and rebuildable per-tenant projections without becoming another database.",
  },
  {
    question: "Is AllSource the same product as ArcGIS AllSource?",
    answer:
      "No. AllSource Event Store is developer infrastructure published at all-source.xyz and github.com/all-source-os/all-source. ArcGIS AllSource is Esri intelligence-analysis software. The products are unrelated.",
  },
  {
    question: "What is the difference between AllSource Core and AllSource Prime?",
    answer:
      "Core is the durable event-store database. Prime is the agent-memory engine that builds graph, vector, and temporal retrieval over event-backed memory. Prime can use Core for persistence; Core does not require Prime.",
  },
  {
    question: "Is AllSource a vector database?",
    answer:
      "No. AllSource Core is an event store. Prime includes vector retrieval as one part of agent memory, alongside graph relationships, temporal context, and source-event provenance.",
  },
  {
    question: "Does AllSource require PostgreSQL?",
    answer:
      "No. Core stores events and event-sourced operational metadata. Query Service derives tenant-scoped reads from Core, so current AllSource services require no PostgreSQL instance.",
  },
  {
    question: "Are AllSource MCP and Prime MCP the same connector?",
    answer:
      "No. The event-store connector exposes 45 read-only tools, 55 by default, 64 with control-plane access, and 73 with system administration. Prime has a separate memory-oriented registry with 19 prime_* tools and 27 when optional inbox and hound modules are enabled.",
  },
] as const;
