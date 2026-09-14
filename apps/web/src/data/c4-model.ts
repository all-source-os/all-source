// ---------------------------------------------------------------------------
// AllSource C4 model — curated, hand-authored single source of truth for the
// public /architecture diagram. Covers the first two C4 layers only:
//   • Level 1 (System Context) — actors, the AllSource platform as ONE system,
//     and the external systems it touches.
//   • Level 2 (Container)      — the deployable containers inside the platform
//     boundary and how they talk.
//
// ACCURACY RULES (must hold — see CLAUDE.md "Architecture Facts"):
//   • Core IS the durable database: WAL (CRC32 + fsync) + Parquet (Snappy) +
//     DashMap (in-memory reads). NEVER described as in-memory-only / a cache /
//     non-durable.
//   • Core stores events and event-sourced operational metadata. Current
//     services have zero PostgreSQL dependency.
//   • The 5 Fly.io backends are: allsource-core, allsource-query,
//     allsource-control-plane, allsource-auth, allsource-prime.
//     The web frontend is on Fly.io (www.all-source.xyz).
//   • Topology: Clients → Control Plane / Query Service → Core. Control Plane
//     owns public auth + billing; Query Service owns tenant-facing read paths.
//
// When unsure about a relationship, OMIT it rather than guess.
// ---------------------------------------------------------------------------

/** C4 element classification — drives node colour + legend. */
export type C4ElementType =
  | "person" // an actor / role outside the system
  | "external" // an external software system we don't own
  | "system" // the AllSource platform as one box (L1 only)
  | "container" // a deployable unit inside the platform (L2)
  | "datastore"; // a place data lives (L2)

export interface C4Node {
  id: string;
  name: string;
  type: C4ElementType;
  /** Tech stack one-liner (e.g. "Rust / Axum"). */
  tech?: string;
  /** One-line responsibility shown in the bubble subtitle + panel. */
  description: string;
  /** Longer bullet responsibilities for the detail panel. */
  responsibilities?: string[];
  /** Part of the AI-agent product suite built on the event store. */
  agentSuite?: boolean;
  /** Deployed as one of the 5 Fly.io backend apps. */
  fly?: boolean;
  /** Deployed on Vercel. */
  vercel?: boolean;
}

export interface C4Edge {
  source: string;
  target: string;
  /** Directed, human-readable relationship label. */
  label: string;
}

export interface C4Level {
  id: "context" | "container";
  title: string;
  subtitle: string;
  nodes: C4Node[];
  edges: C4Edge[];
}

// ===========================================================================
// LEVEL 1 — SYSTEM CONTEXT
// AllSource is ONE system. Actors on the left, external systems on the right.
// ===========================================================================

const contextNodes: C4Node[] = [
  // --- the system ---
  {
    id: "allsource",
    name: "AllSource Platform",
    type: "system",
    tech: "Durable event store + agent products",
    description:
      "A durable Rust event store (WAL + Parquet + DashMap) and the suite of AI-agent products built on top of it.",
    responsibilities: [
      "Store every fact as an immutable, replayable event — durable across restarts.",
      "Serve agent memory (Prime), event-sourced task tracking (chronis), and SDKs.",
      "Multi-tenant API gateway with auth, billing, and quota enforcement.",
    ],
  },

  // --- people / actors ---
  {
    id: "ai-agent",
    name: "AI Agent",
    type: "person",
    description: "Claude (Desktop / Code) or any MCP client that reads and writes durable memory.",
    responsibilities: [
      "Calls prime_* MCP tools to remember, recall, and reason over a knowledge graph.",
      "Auto-injects a compressed memory index at conversation start.",
    ],
    agentSuite: true,
  },
  {
    id: "developer",
    name: "Developer",
    type: "person",
    description: "Builds on AllSource via the language SDKs and the chronis task CLI.",
    responsibilities: [
      "Ingests and queries events through the Rust / Go / Python / TypeScript SDKs.",
      "Tracks event-sourced work with the chronis (cn) CLI.",
    ],
  },
  {
    id: "end-user",
    name: "End User",
    type: "person",
    description: "Signs in to the web dashboard to inspect tenants, usage, and the memory graph.",
    responsibilities: ["OAuth sign-in, browses projections / events / billing in the dashboard."],
  },

  // --- external systems ---
  {
    id: "claude-desktop",
    name: "Claude Desktop / Code",
    type: "external",
    description: "MCP host that loads the Prime / chronis MCP servers over stdio.",
  },
  {
    id: "github",
    name: "GitHub",
    type: "external",
    description: "Source, releases, and the package registry for SDK / crate artifacts.",
  },
  {
    id: "oauth",
    name: "Google OAuth",
    type: "external",
    description: "Social identity provider for dashboard sign-in.",
  },
  {
    id: "fly",
    name: "Fly.io",
    type: "external",
    description: "Hosts the five AllSource backend apps as deployed machines.",
  },
  {
    id: "vercel",
    name: "Vercel",
    type: "external",
    description: "Hosts the Next.js web frontend on Fly.io at www.all-source.xyz.",
  },
  {
    id: "billing",
    name: "Billing (x402 / LemonSqueezy)",
    type: "external",
    description: "Payments and subscription webhooks for tenant billing.",
  },
];

const contextEdges: C4Edge[] = [
  { source: "ai-agent", target: "claude-desktop", label: "runs inside" },
  { source: "claude-desktop", target: "allsource", label: "writes & recalls memory via MCP" },
  {
    source: "developer",
    target: "allsource",
    label: "ingests / queries events via SDKs + chronis",
  },
  { source: "end-user", target: "allsource", label: "manages tenants via dashboard" },
  { source: "end-user", target: "oauth", label: "OAuth sign-in" },
  { source: "oauth", target: "allsource", label: "returns verified identity" },
  { source: "allsource", target: "billing", label: "meters usage, takes payment" },
  { source: "allsource", target: "github", label: "publishes SDKs & releases" },
  { source: "fly", target: "allsource", label: "hosts backend apps" },
  { source: "vercel", target: "allsource", label: "hosts web dashboard" },
];

// ===========================================================================
// LEVEL 2 — CONTAINER
// Inside the AllSource platform boundary: deployable containers + datastores.
// ===========================================================================

const containerNodes: C4Node[] = [
  // --- the database (the heart) ---
  {
    id: "core",
    name: "AllSource Core",
    type: "container",
    tech: "Rust / Axum",
    description:
      "The database. A purpose-built, durable event store — the source of truth for all event data.",
    responsibilities: [
      "WAL: append-only log with CRC32 checksums and configurable fsync (crash recovery).",
      "Parquet: Snappy-compressed columnar cold storage, flushed periodically.",
      "DashMap: in-memory concurrent index for ~12µs reads, ~469K events/sec.",
      "Hosts Prime as an embedded subsystem; exposes /api/v1 events, projections, schemas, snapshots.",
    ],
    fly: true,
  },

  // --- gateway / planes ---
  {
    id: "query-service",
    name: "Query Service",
    type: "container",
    tech: "Elixir / Phoenix",
    description:
      "Stateless read plane over Core — tenant-scoped HTTP, realtime, analytics, and projections.",
    responsibilities: [
      "Serves tenant-scoped event, stream, schema, and projection reads over HTTP.",
      "Broadcasts Core events and projection updates through Phoenix Channels at /ws.",
      "Provides Core-backed and locally computed analytics endpoints with ETS caching.",
      "Maintains rebuildable per-tenant projections and replay jobs; durable data remains in Core.",
    ],
    fly: true,
  },
  {
    id: "control-plane",
    name: "Control Plane",
    type: "container",
    tech: "Go",
    description: "Owns public auth and billing — tenant lifecycle, sign-in, and subscriptions.",
    responsibilities: [
      "Terminates all public authentication (OAuth, anonymous trial keys).",
      "Manages tenants, API keys, and subscription / billing state.",
      "Delegates event-sourced tenant + audit + config writes to Core.",
    ],
    fly: true,
  },
  {
    id: "auth",
    name: "Auth Service",
    type: "container",
    tech: "Rust (better-auth + AllSource adapter)",
    description: "better-auth integration backed by AllSource events.",
    responsibilities: [
      "Provides the better-auth adapter that persists sessions / accounts as AllSource events.",
    ],
    fly: true,
  },

  // --- agent-product suite ---
  {
    id: "prime",
    name: "Prime",
    type: "container",
    tech: "Rust (embedded in Core)",
    description:
      "Agent memory engine — graph + vector + temporal recall, materialized as prime.* events in Core.",
    responsibilities: [
      "Knowledge graph (nodes / edges), HNSW vector search, and time-travel queries.",
      "Every mutation is a prime.* event in Core's WAL — durable and replayable.",
      "Not a separate database: it embeds Core's WAL + Parquet + DashMap engine.",
    ],
    agentSuite: true,
  },
  {
    id: "prime-mcp",
    name: "prime-mcp",
    type: "container",
    tech: "Rust (stdio + HTTP MCP)",
    description:
      "MCP server exposing Prime's prime_* tools to Claude; optionally syncs to a tenant's hosted Core.",
    responsibilities: [
      "Serves prime_* tools over stdio (local) and HTTP.",
      "Auto-injects a compressed memory index at conversation start.",
      "Can sync prime.* events to a tenant's hosted Core via the Query Service.",
    ],
    agentSuite: true,
    fly: true, // allsource-prime
  },
  {
    id: "mcp-elixir",
    name: "mcp-server-elixir",
    type: "container",
    tech: "Elixir",
    description: "Elixir MCP server fronting the platform's event APIs for LLM tool use.",
    responsibilities: ["Exposes AllSource event operations as MCP tools via the Query Service."],
    agentSuite: true,
  },
  {
    id: "chronis",
    name: "chronis (cn)",
    type: "container",
    tech: "Rust CLI",
    description:
      "Event-sourced agent task CLI — tasks, dependencies, and claims as durable events.",
    responsibilities: [
      "Models tasks / dependencies as events; syncs to a tenant's Core for shared queues.",
      "Auto-derives a per-thread claim identity for agent orchestration.",
    ],
    agentSuite: true,
  },
  {
    id: "sdks",
    name: "SDKs",
    type: "container",
    tech: "Rust · Go · Python · TypeScript",
    description: "Standalone HTTP client libraries for ingesting and querying events.",
    responsibilities: [
      "Typed clients for the /api/v1 event API, used by developer applications.",
      "Leaf components: depend on nothing internal, talk to the Query Service over HTTP.",
    ],
    agentSuite: true,
  },

  // --- frontend ---
  {
    id: "web",
    name: "Web Dashboard",
    type: "container",
    tech: "Next.js 16 / React 19 (Vercel)",
    description:
      "Public marketing site + authenticated dashboard, including the Prime memory graph.",
    responsibilities: [
      "Marketing pages and the tenant dashboard (events, projections, billing, memory graph).",
      "Calls the Query Service for data; auth flows go through the Control Plane.",
    ],
    vercel: true,
  },

  // --- datastores ---
  {
    id: "core-storage",
    name: "Core WAL + Parquet",
    type: "datastore",
    tech: "WAL (CRC32 + fsync) + Parquet (Snappy)",
    description: "Durable event storage — the system of record for every event.",
    responsibilities: [
      "Write-ahead log guarantees durability and crash recovery.",
      "Parquet columnar files hold compressed historical events.",
    ],
  },
];

const containerEdges: C4Edge[] = [
  // gateway → core
  {
    source: "query-service",
    target: "core",
    label: "queries events; builds read models; subscribes to live stream",
  },
  { source: "core", target: "core-storage", label: "persists events (WAL + Parquet)" },

  // control plane
  { source: "control-plane", target: "core", label: "delegates tenant / audit / config events" },

  // prime is embedded in core
  { source: "prime", target: "core", label: "embeds (prime.* events)" },

  // agent suite
  { source: "prime-mcp", target: "prime", label: "exposes prime_* tools" },
  { source: "prime-mcp", target: "query-service", label: "syncs prime.* to tenant Core" },
  { source: "chronis", target: "query-service", label: "syncs task events" },
  { source: "mcp-elixir", target: "query-service", label: "event tool calls" },
  { source: "sdks", target: "query-service", label: "ingest / query via HTTP" },

  // web
  { source: "web", target: "query-service", label: "reads dashboard data" },
  { source: "web", target: "control-plane", label: "auth + billing flows" },

  // auth
  { source: "auth", target: "core", label: "persists sessions as events" },
];

export const C4_LEVELS: Record<"context" | "container", C4Level> = {
  context: {
    id: "context",
    title: "System Context",
    subtitle:
      "Who uses AllSource and what it connects to — the platform as one durable event store with an agent-product suite on top.",
    nodes: contextNodes,
    edges: contextEdges,
  },
  container: {
    id: "container",
    title: "Containers",
    subtitle:
      "Inside the platform boundary: the deployable services and datastores, and how they talk. Core is the durable database; the agent-suite products (highlighted) are built on top of it.",
    nodes: containerNodes,
    edges: containerEdges,
  },
};

/** Colour-by-C4-element-type — plain hex (canvas can't resolve CSS vars). */
export const C4_TYPE_COLORS: Record<C4ElementType, string> = {
  person: "#10b981", // emerald
  external: "#64748b", // slate
  system: "#6366f1", // indigo
  container: "#0ea5e9", // sky
  datastore: "#f59e0b", // amber
};

export const C4_TYPE_LABELS: Record<C4ElementType, string> = {
  person: "Person / actor",
  external: "External system",
  system: "AllSource system",
  container: "Container",
  datastore: "Data store",
};
