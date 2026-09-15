// ---------------------------------------------------------------------------
// AllSource Ecosystem model — curated, hand-authored single source of truth for
// the public /ecosystem diagram. This is the AGENT-FIRST companion to the C4
// /architecture page: that page = how the platform is built; this page = what
// an AI agent can DO with it and exactly how to wire each piece.
//
// It is a BIPARTITE graph:
//   • capability nodes  — what an AI agent can DO (remember, recall, build a
//     graph, track tasks, ingest/query events, stream, start with no signup,
//     use it from Claude Desktop).
//   • app/endpoint nodes — the PUBLIC, agent-usable apps / products / endpoints
//     that provide those abilities (prime-mcp, the DXT, chronis, the SDKs, the
//     public HTTP/WS API, /connect, crates.io, GitHub).
// Edges go capability -> app/endpoint ("this is how you get this ability").
//
// ACCURACY RULES (the #1 failure mode is inaccuracy — verify, don't invent):
//   • PUBLIC surface ONLY. Public hosts: api.all-source.xyz (Control Plane
//     gateway), allsource-query.fly.dev (Query Service), www.all-source.xyz
//     (dashboard). NEVER show *.internal hosts or internal-only services.
//   • Prime runs two ways: the local stdio binary (`cargo install
//     allsource-prime`, optionally `--sync-to https://api.all-source.xyz
//     --api-key <key>`), or the hosted MCP URL
//     https://api.all-source.xyz/api/v1/prime/mcp with a bearer API key.
//   • crates.io publishes the Rust crates: allsource-prime (the MCP binary),
//     allsource (Rust SDK), chronis (the `cn` CLI binary). The TS SDK is on
//     npm as `@allsourcedev/client`; Python / Go SDKs are GitHub-registry only
//     — never print a fake `pip install`/`go get` from a registry.
//   • The DXT for Claude Desktop ships via GitHub releases:
//     github.com/all-source-os/all-source/releases/latest/download/allsource-prime.dxt
//   • prime_* tool names are the REAL tool names from apps/prime-mcp/src.
//
// Verified against (2026-06): apps/prime-mcp/src/tools.rs + dispatch.rs (tool
// names + the real clap flags), apps/web/src/lib/integrations.ts (the canonical
// MCP-config envelope), apps/web/src/app/(marketing)/sdks/page.tsx (the real
// SDK channels), apps/web/content/allsource-as-cms-from-claude-desktop.mdx +
// docs/proposals/AGENT_DRIVEN_PRIME_ONBOARDING.md (anonymous-trial + /connect),
// apps/web/content/audit-trails-soc2-event-sourcing.mdx (the events curl).
//
// When unsure about an endpoint or command, OMIT it rather than guess.
// ---------------------------------------------------------------------------

/** The public gateway Prime / SDKs / curl all talk to. Never Core directly. */
export const GATEWAY_URL = "https://api.all-source.xyz";

/** crates.io install for the local Prime MCP stdio binary. */
export const PRIME_INSTALL = "cargo install allsource-prime";

/** GitHub-releases download for the Claude Desktop one-click extension. */
export const DXT_URL =
  "https://github.com/all-source-os/all-source/releases/latest/download/allsource-prime.dxt";

/** Canonical hosted MCP config (Claude Desktop / Cursor / Windsurf / VS Code
 *  all share this `mcpServers` shape). Mirrors integrations.ts mcpServersJson. */
export const HOSTED_MCP_CONFIG = `{
  "mcpServers": {
    "prime": {
      "command": "allsource-prime",
      "args": [
        "--data-dir", "~/.prime/memory",
        "--auto-inject",
        "--sync-to", "${GATEWAY_URL}",
        "--api-key", "<YOUR_API_KEY>"
      ]
    }
  }
}`;

/** Local-only MCP config — same binary, no account, memory stays on disk. */
export const LOCAL_MCP_CONFIG = `{
  "mcpServers": {
    "prime": {
      "command": "allsource-prime",
      "args": ["--data-dir", "~/.prime/memory", "--auto-inject"]
    }
  }
}`;

// ===========================================================================
// Node + edge types
// ===========================================================================

/** Visual class. Drives colour + shape: capabilities are one visual family,
 *  apps/endpoints another (further sub-typed by `kind` for the filter). */
export type EcosystemNodeType = "capability" | "app";

/** App/endpoint sub-kind — drives the kind filter + a small badge. */
export type AppKind = "mcp" | "cli" | "sdk" | "api" | "package" | "service";

/** A copy-paste-able command/config/URL shown in the detail panel. */
export interface CodeSnippet {
  /** Short label above the block, e.g. "Install" or "MCP config (~/.config…)". */
  label: string;
  /** The exact text to copy. Must be real + public. */
  code: string;
  /** Syntax hint (only affects styling). */
  lang?: "bash" | "json" | "toml" | "http";
}

/** A working external link shown in the detail panel. */
export interface NodeLink {
  label: string;
  href: string;
}

export interface EcosystemNode {
  id: string;
  name: string;
  type: EcosystemNodeType;
  /** One-line summary shown in the bubble subtitle + panel header. */
  summary: string;
  /** App/endpoint only — its sub-kind (for filter + badge). */
  kind?: AppKind;
  /** App/endpoint only — "what your agent gets" one-liner. */
  agentGets?: string;
  /** App/endpoint only — copy-paste install / MCP-config / curl blocks. */
  snippets?: CodeSnippet[];
  /** App/endpoint only — docs / source / crates.io / release links. */
  links?: NodeLink[];
  /** Capability only — the real prime_* tool names that back it, if any. */
  tools?: string[];
}

export interface EcosystemEdge {
  /** capability id */
  source: string;
  /** app/endpoint id */
  target: string;
  /** Directed relation: how the capability is delivered by the app. */
  label: string;
}

// ===========================================================================
// APP / ENDPOINT NODES — the PUBLIC, agent-usable surface
// ===========================================================================

const appNodes: EcosystemNode[] = [
  {
    id: "prime-mcp",
    name: "prime-mcp",
    type: "app",
    kind: "mcp",
    summary:
      "Local MCP server that gives your agent durable memory — a knowledge graph, vector recall, and time-travel, exposed as prime_* tools.",
    agentGets:
      "A set of prime_* tools your agent calls to remember facts, recall by meaning, and reason over a knowledge graph — local-first, optionally synced to your tenant.",
    snippets: [
      { label: "Install (crates.io)", code: PRIME_INSTALL, lang: "bash" },
      {
        label: "Hosted MCP config — syncs to your tenant",
        code: HOSTED_MCP_CONFIG,
        lang: "json",
      },
      {
        label: "Local-only MCP config — no account, memory on disk",
        code: LOCAL_MCP_CONFIG,
        lang: "json",
      },
    ],
    links: [
      { label: "crates.io · allsource-prime", href: "https://crates.io/crates/allsource-prime" },
      { label: "Install in your tools", href: "/install" },
      { label: "MCP setup docs", href: "/docs/prime/mcp" },
    ],
  },
  {
    id: "dxt",
    name: "Claude Desktop DXT",
    type: "app",
    kind: "mcp",
    summary:
      "One-click Claude Desktop extension. Download the .dxt, drag it into Claude Desktop, paste an API key — no terminal.",
    agentGets:
      "The fastest no-terminal install of Prime: Claude Desktop writes the MCP config for you and your agent has prime_* tools after a restart.",
    snippets: [
      {
        label: "Download (GitHub releases, ~37 MB)",
        code: DXT_URL,
        lang: "http",
      },
    ],
    links: [
      { label: "Download allsource-prime.dxt", href: DXT_URL },
      {
        label: "CMS-from-Claude-Desktop install protocol",
        href: "/blog/allsource-as-cms-from-claude-desktop",
      },
    ],
  },
  {
    id: "chronis",
    name: "chronis (cn)",
    type: "app",
    kind: "cli",
    summary:
      "Event-sourced task CLI. Your agent tracks its tasks, dependencies, and claims as durable events — local-first, syncable to your tenant Core.",
    agentGets:
      "A durable, event-sourced task queue your agent can drive from the shell (`cn`) with auto-derived per-thread claim identity for orchestration.",
    snippets: [
      {
        label: "Install (crates.io) — binary is `cn`",
        code: "cargo install chronis",
        lang: "bash",
      },
    ],
    links: [
      { label: "crates.io · chronis", href: "https://crates.io/crates/chronis" },
      { label: "GitHub source", href: "https://github.com/all-source-os/all-source" },
    ],
  },
  {
    id: "rust-sdk",
    name: "Rust SDK",
    type: "app",
    kind: "sdk",
    summary:
      "First-class typed client on crates.io. Async (tokio) HTTP QueryClient for the gateway, plus an in-process EventStore.",
    agentGets:
      "Programmatic ingest + query from Rust agents — the only SDK published to crates.io.",
    snippets: [
      { label: "Add to your crate (crates.io)", code: "cargo add allsource", lang: "bash" },
    ],
    links: [
      { label: "crates.io · allsource", href: "https://crates.io/crates/allsource" },
      {
        label: "Source",
        href: "https://github.com/all-source-os/all-source/tree/main/sdks/rust",
      },
      { label: "All SDKs", href: "/sdks" },
    ],
  },
  {
    id: "other-sdks",
    name: "TS · Python · Go SDKs",
    type: "app",
    kind: "sdk",
    summary:
      "Typed gateway clients for TypeScript, Python, and Go. TypeScript ships on npm as `@allsourcedev/client`; Python and Go are GitHub-registry only (NOT PyPI, NOT a Go proxy module), version-locked to the gateway.",
    agentGets:
      "Ingest + query from JS/TS, Python, or Go agents. `bun add @allsourcedev/client` for TypeScript; Python and Go install straight from the GitHub monorepo, not a public PyPI/proxy package.",
    snippets: [
      {
        label: "TypeScript (npm)",
        code: "bun add @allsourcedev/client",
        lang: "bash",
      },
      {
        label: "Python (GitHub, via uv/pip)",
        code: "uv pip install git+https://github.com/all-source-os/all-source#subdirectory=sdks/python-client",
        lang: "bash",
      },
      {
        label: "Go (Go modules from source)",
        code: "go get github.com/all-source-os/all-source/sdks/go",
        lang: "bash",
      },
    ],
    links: [
      { label: "SDKs — why not npm/PyPI", href: "/sdks" },
      {
        label: "TypeScript source",
        href: "https://github.com/all-source-os/all-source/tree/main/sdks/typescript",
      },
      {
        label: "Python source",
        href: "https://github.com/all-source-os/all-source/tree/main/sdks/python-client",
      },
      { label: "Go source", href: "https://github.com/all-source-os/all-source/tree/main/sdks/go" },
    ],
  },
  {
    id: "events-ingest",
    name: "POST /api/v1/events",
    type: "app",
    kind: "api",
    summary:
      "Public ingest endpoint on the gateway. Append an immutable event with a Bearer token — durable in Core's WAL the moment it returns 200.",
    agentGets:
      "Any agent that can make an HTTP call can write a fact into a durable, append-only event log — no SDK required.",
    snippets: [
      {
        label: "Ingest an event (curl)",
        code: `curl -X POST ${GATEWAY_URL}/api/v1/events \\
  -H "Authorization: Bearer $API_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{
    "event_type": "agent.observation",
    "entity_id": "session-1",
    "payload": { "note": "remembered something durable" }
  }'`,
        lang: "bash",
      },
    ],
    links: [
      { label: "Connecting without an SDK", href: "/blog/connecting-without-an-sdk" },
      { label: "API reference", href: "/docs/api" },
    ],
  },
  {
    id: "events-query",
    name: "GET /api/v1/events/query",
    type: "app",
    kind: "api",
    summary:
      "Query past events on the gateway — filter by entity, type, and time. Reconstruct exact state at any point (time-travel).",
    agentGets:
      "Your agent reads its own history back: a full, ordered timeline it can replay to reconstruct state at any moment.",
    snippets: [
      {
        label: "Query an entity timeline (curl)",
        code: `curl "${GATEWAY_URL}/api/v1/events/query?\\
entity_id=session-1&\\
event_type=agent.observation&\\
limit=100&order=desc" \\
  -H "Authorization: Bearer $API_KEY"`,
        lang: "bash",
      },
    ],
    links: [{ label: "API reference", href: "/docs/api" }],
  },
  {
    id: "prime-graph",
    name: "GET /api/v1/prime/graph",
    type: "app",
    kind: "api",
    summary:
      "Read your agent's materialized memory graph (nodes + edges) over HTTP, behind the gateway. Powers the dashboard Memory tab.",
    agentGets:
      "The same knowledge graph the prime_* tools build, readable as JSON — so any client can render or reason over your agent's memory.",
    snippets: [
      {
        label: "Read the memory graph (curl)",
        code: `curl "${GATEWAY_URL}/api/v1/prime/graph" \\
  -H "Authorization: Bearer $API_KEY"`,
        lang: "bash",
      },
    ],
    links: [
      { label: "Prime HTTP API docs", href: "/docs/prime/http" },
      { label: "Memory graph (dashboard)", href: "/dashboard/memory" },
    ],
  },
  {
    id: "events-stream",
    name: "wss · /api/v1/events/stream",
    type: "app",
    kind: "api",
    summary:
      "Subscribe to live events over WebSocket on the gateway. Your agent reacts to changes as they happen.",
    agentGets:
      "A live tail of the event log: your agent gets pushed new events in real time instead of polling.",
    snippets: [
      {
        label: "Stream live events (wscat)",
        code: `wscat -c "wss://api.all-source.xyz/api/v1/events/stream?consumer_id=my-agent" \\
  -H "Authorization: Bearer $API_KEY"`,
        lang: "bash",
      },
    ],
    links: [
      { label: "Connecting without an SDK", href: "/blog/connecting-without-an-sdk" },
      { label: "API reference", href: "/docs/api" },
    ],
  },
  {
    id: "anon-trial",
    name: "POST /api/v1/agents/anonymous-trial",
    type: "app",
    kind: "api",
    summary:
      "Mint a low-quota, time-limited API key with zero signup. The agent calls it itself — no human sign-in to get started.",
    agentGets:
      "An agent can self-serve a working API key from a single curl — push events immediately, claim the trial into a real account later via /connect.",
    snippets: [
      {
        label: "Mint a trial key — no signup (curl)",
        code: `curl -sS -X POST ${GATEWAY_URL}/api/v1/agents/anonymous-trial \\
  -H 'Content-Type: application/json' \\
  -d '{"agent_name": "claude-desktop"}'`,
        lang: "bash",
      },
    ],
    links: [
      { label: "Claim a trial into your tenant", href: "/connect" },
      {
        label: "Agent-driven onboarding (proposal)",
        href: "https://github.com/all-source-os/all-source/blob/main/docs/proposals/AGENT_DRIVEN_PRIME_ONBOARDING.md",
      },
    ],
  },
  {
    id: "connect",
    name: "/connect deep-link",
    type: "app",
    kind: "service",
    summary:
      "Hosted mint + claim flow. A signed-in human mints an API key, or claims an agent's anonymous-trial events into their tenant.",
    agentGets:
      "The bridge from a self-served trial to a real tenant: open /connect?claim=<token> to migrate the agent's events into an owned account.",
    snippets: [
      {
        label: "Open the hosted mint / claim flow",
        code: "https://www.all-source.xyz/connect?source=ecosystem",
        lang: "http",
      },
    ],
    links: [
      { label: "Mint an API key", href: "/connect?source=ecosystem" },
      { label: "Install hub", href: "/install" },
    ],
  },
  {
    id: "cratesio",
    name: "crates.io",
    type: "app",
    kind: "package",
    summary:
      "The public Rust registry that ships the agent-usable binaries + the Rust SDK: allsource-prime, chronis, allsource (+ supporting crates).",
    agentGets:
      "One `cargo install` away from the Prime MCP server and the chronis task CLI — the real public channel for the Rust pieces.",
    snippets: [
      {
        label: "The three agent-facing crates",
        code: `cargo install allsource-prime   # Prime MCP server (binary)
cargo install chronis           # task CLI (binary: cn)
cargo add allsource             # Rust SDK (library)`,
        lang: "bash",
      },
    ],
    links: [
      { label: "allsource-prime", href: "https://crates.io/crates/allsource-prime" },
      { label: "chronis", href: "https://crates.io/crates/chronis" },
      { label: "allsource (SDK)", href: "https://crates.io/crates/allsource" },
    ],
  },
  {
    id: "github",
    name: "GitHub · all-source",
    type: "app",
    kind: "package",
    summary:
      "Source, releases (the DXT), and the registry for the TS / Python / Go SDKs. Everything is open and readable.",
    agentGets:
      "The canonical source + release channel: the DXT bundle, the GitHub-registry SDKs, and the full open-source code your agent runs.",
    snippets: [],
    links: [
      { label: "Repository", href: "https://github.com/all-source-os/all-source" },
      {
        label: "Latest releases (DXT)",
        href: "https://github.com/all-source-os/all-source/releases/latest",
      },
      {
        label: "Discussions / community",
        href: "https://github.com/all-source-os/all-source/discussions",
      },
    ],
  },
];

// ===========================================================================
// CAPABILITY NODES — what an AI agent can DO
// ===========================================================================

const capabilityNodes: EcosystemNode[] = [
  {
    id: "cap-remember",
    name: "Remember across sessions",
    type: "capability",
    summary:
      "Persist facts, entities, and decisions that survive restarts and outlive a single conversation.",
    tools: ["prime_add_node", "prime_embed"],
  },
  {
    id: "cap-recall",
    name: "Search its memory semantically",
    type: "capability",
    summary:
      "Find what it knows by meaning — hybrid vector + graph + temporal recency recall, not just keyword match.",
    tools: ["prime_recall", "prime_similar", "prime_search"],
  },
  {
    id: "cap-graph",
    name: "Build a knowledge graph",
    type: "capability",
    summary:
      "Model nodes and edges, then traverse them — neighbours, shortest paths, multi-hop reasoning over connected facts.",
    tools: ["prime_add_node", "prime_add_edge", "prime_neighbors", "prime_shortest_path"],
  },
  {
    id: "cap-tasks",
    name: "Track its tasks as events",
    type: "capability",
    summary:
      "Drive a durable, event-sourced task queue — tasks, dependencies, and claims — from the shell via chronis (`cn`).",
  },
  {
    id: "cap-ingest",
    name: "Ingest events into a durable store",
    type: "capability",
    summary:
      "Append immutable facts to a write-ahead-logged event store the moment they happen — durable across restarts.",
  },
  {
    id: "cap-query",
    name: "Query past events · time-travel",
    type: "capability",
    summary:
      "Read history back, filtered by entity / type / time, and reconstruct exact state at any past moment.",
    tools: ["prime_history", "prime_node_provenance"],
  },
  {
    id: "cap-stream",
    name: "Stream live events",
    type: "capability",
    summary: "Subscribe over WebSocket and react to new events in real time instead of polling.",
  },
  {
    id: "cap-zero-signup",
    name: "Start with zero signup",
    type: "capability",
    summary:
      "Self-serve a working API key from a single call — no human sign-in — then claim it into a real account later.",
  },
  {
    id: "cap-claude-desktop",
    name: "Use it from Claude Desktop",
    type: "capability",
    summary:
      "Wire memory into Claude Desktop in one click via the DXT — or any MCP client with the shared config.",
  },
];

// ===========================================================================
// EDGES — capability -> app/endpoint(s) that provide it (bipartite, directed)
// ===========================================================================

const edges: EcosystemEdge[] = [
  // Remember across sessions
  { source: "cap-remember", target: "prime-mcp", label: "via prime_add_node / prime_embed" },
  { source: "cap-remember", target: "events-ingest", label: "or write raw events" },

  // Semantic recall
  { source: "cap-recall", target: "prime-mcp", label: "via prime_recall MCP tool" },
  { source: "cap-recall", target: "prime-graph", label: "read graph over HTTP" },

  // Knowledge graph
  { source: "cap-graph", target: "prime-mcp", label: "via prime_add_edge / prime_neighbors" },
  { source: "cap-graph", target: "prime-graph", label: "read materialized graph" },

  // Tasks
  { source: "cap-tasks", target: "chronis", label: "install the cn CLI" },

  // Ingest
  { source: "cap-ingest", target: "events-ingest", label: "POST /api/v1/events" },
  { source: "cap-ingest", target: "rust-sdk", label: "typed client (crates.io)" },
  { source: "cap-ingest", target: "other-sdks", label: "typed client (GitHub registry)" },

  // Query / time-travel
  { source: "cap-query", target: "events-query", label: "GET /api/v1/events/query" },
  { source: "cap-query", target: "prime-mcp", label: "via prime_history" },

  // Stream
  { source: "cap-stream", target: "events-stream", label: "wss stream" },

  // Zero signup
  { source: "cap-zero-signup", target: "anon-trial", label: "mint a trial key" },
  { source: "cap-zero-signup", target: "connect", label: "claim into a tenant" },

  // Claude Desktop
  { source: "cap-claude-desktop", target: "dxt", label: "one-click install" },
  { source: "cap-claude-desktop", target: "prime-mcp", label: "stdio MCP config" },

  // Distribution channels (every install ultimately comes from one of these)
  { source: "cap-remember", target: "cratesio", label: "cargo install" },
  { source: "cap-tasks", target: "cratesio", label: "cargo install chronis" },
  { source: "cap-claude-desktop", target: "github", label: "DXT via releases" },
  { source: "cap-ingest", target: "github", label: "SDK source / registry" },
];

export const ECOSYSTEM_NODES: EcosystemNode[] = [...capabilityNodes, ...appNodes];
export const ECOSYSTEM_EDGES: EcosystemEdge[] = edges;

// ---------------------------------------------------------------------------
// Presentation constants — colours (plain hex; canvas can't resolve CSS vars).
// ---------------------------------------------------------------------------

/** Capability bubbles vs each app-kind get a distinct colour for the legend. */
export const NODE_COLORS: Record<"capability" | AppKind, string> = {
  capability: "#a855f7", // purple — what the agent can DO
  mcp: "#0ea5e9", // sky — MCP servers / extensions
  cli: "#22c55e", // green — command-line tools
  sdk: "#f59e0b", // amber — language SDKs
  api: "#ef4444", // red — HTTP/WS endpoints
  package: "#64748b", // slate — registries
  service: "#ec4899", // pink — hosted flows
};

export const KIND_LABELS: Record<AppKind, string> = {
  mcp: "MCP server",
  cli: "CLI",
  sdk: "SDK",
  api: "API endpoint",
  package: "Registry",
  service: "Hosted flow",
};

/** Colour for a node (capability colour, or its app-kind colour). */
export function colorForNode(node: EcosystemNode): string {
  if (node.type === "capability") return NODE_COLORS.capability;
  return NODE_COLORS[node.kind ?? "service"];
}

/** All app-kinds present, for the kind filter. */
export const APP_KINDS: AppKind[] = ["mcp", "cli", "sdk", "api", "package", "service"];
