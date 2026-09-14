/**
 * Per-competitor data for the `/vs/<slug>` comparison pages.
 *
 * DRY contract: the three pages (`mem0`, `letta`, `zep`) are thin shells over
 * `<ComparisonTable>` + this map. Change a claim here and all pages update.
 *
 * Claim discipline (see CLAUDE.md + prompt 013):
 *   - AllSource cells are sourced from product facts: durable WAL+Parquet event
 *     store, 11.9μs p99 Core indexed reads, 469K events/sec reference batch
 *     ingest, Apache-2.0 Core/self-host, x402 per-call
 *     pricing, 55 default tenant MCP tools (73 with fleet controls). These are
 *     defensible against our own docs.
 *   - Competitor cells mirror the homepage matrix (`sections/social-proof.tsx`)
 *     where that matrix took a position, and otherwise read "unknown" / "varies"
 *     rather than inventing a number. NEVER fabricate competitor figures.
 *   - The homepage matrix asserted: temporal `as_of` — zep yes, mem0/letta no;
 *     full event provenance / compressed index / offline-embedded — AllSource
 *     only; Apache-2.0 self-host — mem0 yes, letta/zep no.
 */

/** A table cell. Booleans render as check/cross; strings render verbatim. */
export type Cell = "yes" | "no" | "partial" | "unknown" | "varies" | (string & {});

export type ComparisonRow = {
  feature: string;
  allsource: Cell;
  competitor: Cell;
  /** One honest sentence of context shown under the feature name. */
  note?: string;
};

export type FaqEntry = { question: string; answer: string };

export type Competitor = {
  slug: "mem0" | "letta" | "zep" | "stoolap";
  /** Display name used in H1, table header, metadata. */
  name: string;
  /** One-line, defensible verdict shown under the H1. */
  verdict: string;
  /** Meta description for `constructMetadata`. */
  metaDescription: string;
  /** "Pick AllSource if…" bullets. */
  pickAllsource: string[];
  /** "Pick <Competitor> if…" bullets — kept honest, never disparaging. */
  pickCompetitor: string[];
  rows: ComparisonRow[];
  faqs: FaqEntry[];
  /**
   * OPTIONAL honest framing block, rendered above the table when present.
   * Used when the competitor is a *different category* of product (e.g.
   * stoolap, an embedded SQL DBMS) rather than another agent-memory tool in
   * AllSource's lane. Absent for mem0/letta/zep, so their pages are unchanged.
   */
  category?: { heading: string; body: string };
  /**
   * OPTIONAL override for the "self-host" CTA button label. Defaults to the
   * Apache-2.0 wording; AllSource community is Apache-2.0 (enterprise BSL 1.1).
   */
  selfHostLabel?: string;
  /**
   * OPTIONAL footnote shown under the comparison table, replacing the default
   * "figures from our benchmarks" boilerplate. Lets a different-category page
   * state its honest caveats (e.g. "both are Apache-2.0; numbers are a wash").
   */
  tableNote?: string;
  /**
   * OPTIONAL slug of a long-form companion article under `content/`. When set,
   * the page links to `/blog/<articleSlug>` so the page ↔ article cross-link.
   */
  articleSlug?: string;
  /** Primary product documentation used to verify competitor claims. */
  sources: { label: string; href: string }[];
};

// Rows shared by every competitor — the durable differentiators. Per-competitor
// cells are filled below so each page can diverge where the homepage matrix did.
function baseRows(competitorCells: Record<string, Cell>): ComparisonRow[] {
  return [
    {
      feature: "Durable event store (survives restart)",
      allsource: "WAL + Parquet",
      competitor: competitorCells.durable ?? "unknown",
      note: "AllSource Core is the database: a Rust WAL with CRC32 checks, configurable fsync, and columnar Parquet persistence. Event data survives restarts under the selected durability mode.",
    },
    {
      feature: "Full event provenance / replay",
      allsource: "yes",
      competitor: competitorCells.provenance ?? "no",
      note: "Every change is an immutable event you can replay to reconstruct any past state.",
    },
    {
      feature: "Temporal / as_of queries",
      allsource: "yes",
      competitor: competitorCells.temporal ?? "unknown",
      note: "Point-in-time projections are a first-class query, not a snapshot you have to manage yourself.",
    },
    {
      feature: "Core indexed-read latency",
      allsource: "11.9μs (p99)",
      competitor: competitorCells.latency ?? "unknown",
      note: "Published reference benchmark for Core indexed reads. It does not measure graph, vector, hybrid, or end-to-end agent recall.",
    },
    {
      feature: "Ingestion throughput",
      allsource: "469K events/sec (reference batch benchmark)",
      competitor: competitorCells.throughput ?? "unknown",
      note: "Published Core batch-ingest reference. We do not publish competitor throughput figures we cannot verify.",
    },
    {
      feature: "Offline / embedded mode",
      allsource: "yes",
      competitor: competitorCells.embedded ?? "no",
      note: "Run AllSource in-process as a library — no separate server or network hop required.",
    },
    {
      feature: "MCP tools for AI agents",
      allsource: "55 tenant / 73 fleet",
      competitor: competitorCells.mcp ?? "unknown",
      note: "The default tenant connector exposes 55 tools. Fleet and administrative controls bring the exposed total to 73.",
    },
    {
      feature: "x402 per-call agent payments",
      allsource: "yes",
      competitor: competitorCells.x402 ?? "no",
      note: "Native x402 micropayments let agents pay per call instead of negotiating a seat-based contract.",
    },
    {
      feature: "License / self-host",
      allsource: "Apache-2.0, self-host",
      competitor: competitorCells.license ?? "unknown",
      note: "Self-host Apache-2.0 Core and community components for free; enterprise components use BSL 1.1. Hosted tiers are optional.",
    },
  ];
}

export type CompetitorSlug = "mem0" | "letta" | "zep" | "stoolap";

// Keyed by the literal slug union (not `Record<string, …>`) so lookups like
// `competitors.mem0` are non-optional under noUncheckedIndexedAccess.
export const competitors: Record<CompetitorSlug, Competitor> = {
  mem0: {
    slug: "mem0",
    name: "mem0",
    verdict:
      "mem0 is a memory layer that bolts onto your LLM app. AllSource is a durable event store underneath it — you keep full provenance and time-travel instead of a lossy summary.",
    metaDescription:
      "AllSource vs mem0: compare an event store with durable history, provenance, embedded mode, MCP access, and x402 pricing against a managed LLM memory layer.",
    pickAllsource: [
      "You need every memory write to be auditable and replayable, not summarized away",
      "You want a published microsecond-range Core indexed-read benchmark, not only a remote memory API",
      "You want to embed the store in-process or self-host it for free under Apache-2.0",
      "Your agents pay per call (x402) instead of per seat",
    ],
    pickCompetitor: [
      "You want a managed memory abstraction and are happy to self-host the open-source core",
      "You do not need full event provenance or point-in-time replay",
      "A higher-level 'remember this' API matters more to you than the underlying store",
    ],
    rows: baseRows({
      // Homepage matrix: mem0 is Apache-2.0/self-host = yes; temporal/provenance = no.
      durable: "unknown",
      provenance: "no",
      temporal: "no",
      latency: "unknown",
      throughput: "unknown",
      embedded: "no",
      mcp: "unknown",
      x402: "no",
      license: "Apache-2.0, self-host",
    }),
    faqs: [
      {
        question: "Is mem0 better than AllSource?",
        answer:
          "They solve different layers. mem0 is a managed memory abstraction for LLM apps; AllSource is a durable event store (WAL + Parquet) with full provenance and time-travel queries. AllSource Core's published reference benchmark reports 11.9μs p99 indexed reads; it is not an end-to-end memory-recall benchmark. If you want a hosted 'remember this' abstraction and do not need full provenance, mem0 may be enough.",
      },
      {
        question: "Can I self-host both AllSource and mem0?",
        answer:
          "Yes. AllSource is Apache-2.0 licensed and self-hostable, and mem0's core is open source. AllSource also ships an embedded in-process mode and 55 tenant-scoped MCP tools by default, rising to 73 with fleet controls.",
      },
      {
        question: "Does AllSource keep full event history like mem0?",
        answer:
          "AllSource keeps the complete, immutable event log and lets you replay it to any past state. Memory layers like mem0 typically store distilled memories rather than the full provenance.",
      },
    ],
    sources: [
      { label: "mem0 open-source overview", href: "https://docs.mem0.ai/open-source/overview" },
      { label: "mem0 repository", href: "https://github.com/mem0ai/mem0" },
    ],
  },

  letta: {
    slug: "letta",
    name: "Letta",
    verdict:
      "Letta (formerly MemGPT) gives agents a stateful memory loop. AllSource gives that loop a durable, queryable foundation with full event history and point-in-time replay.",
    metaDescription:
      "AllSource vs Letta (MemGPT): compare a durable event store with provenance, embedded mode, MCP access, and x402 pricing against a stateful agent framework.",
    pickAllsource: [
      "You want a durable store of record under your agents, not just an in-context memory manager",
      "You need point-in-time queries and full replay for audit or debugging",
      "You want a published microsecond-range Core indexed-read benchmark and the option to embed or self-host under Apache-2.0",
      "You want per-call (x402) economics for autonomous agents",
    ],
    pickCompetitor: [
      "You want an opinionated agent runtime with memory management built in",
      "You are standardizing on Letta's agent abstractions and tooling",
      "You do not need a separate durable event store of record",
    ],
    rows: baseRows({
      // Homepage matrix: letta temporal/provenance/Apache-2.0-self-host = no.
      durable: "unknown",
      provenance: "no",
      temporal: "no",
      latency: "unknown",
      throughput: "unknown",
      embedded: "no",
      mcp: "unknown",
      x402: "no",
      license: "unknown",
    }),
    faqs: [
      {
        question: "Is Letta better than AllSource?",
        answer:
          "Letta is an agent framework with a stateful memory loop; AllSource is the durable event store that can sit underneath it. Letta fits teams wanting a batteries-included agent runtime. AllSource fits auditable, replayable storage and time-travel queries. Its published 11.9μs p99 result covers Core indexed reads, not end-to-end agent recall.",
      },
      {
        question: "What is the difference between Letta and AllSource?",
        answer:
          "Letta orchestrates an agent and its working memory. AllSource is infrastructure: a Rust WAL + Parquet event store with full provenance, embedded mode, and 55 tenant-scoped MCP tools by default. They operate at different layers and can be combined.",
      },
      {
        question: "Does AllSource replace Letta?",
        answer:
          "Not directly. AllSource replaces the durable persistence and recall layer; you can keep a framework like Letta on top, or drive AllSource directly over HTTP and MCP.",
      },
    ],
    sources: [
      { label: "Letta documentation", href: "https://docs.letta.com/" },
      { label: "Letta repository", href: "https://github.com/letta-ai/letta" },
    ],
  },

  zep: {
    slug: "zep",
    name: "Zep",
    verdict:
      "Zep adds a temporal memory service for chat history. AllSource makes temporal history a property of the event store: every event is replayable, with embedded and self-hosted deployment options.",
    metaDescription:
      "AllSource vs Zep: compare a durable event store with provenance, embedded mode, MCP access, and x402 pricing against a temporal memory service for LLM apps.",
    pickAllsource: [
      "You want temporal queries across all your data, not only chat memory",
      "You need full event provenance and replay, plus a published microsecond-range Core indexed-read benchmark",
      "You want to embed in-process or self-host the whole stack under Apache-2.0",
      "You want per-call (x402) agent economics out of the box",
    ],
    pickCompetitor: [
      "You specifically want a managed memory service tuned for conversational history",
      "You are happy with a hosted temporal memory API and do not need an embedded store",
      "You do not need a general-purpose event store beyond agent memory",
    ],
    rows: baseRows({
      // Homepage matrix: zep temporal = yes; provenance/compressed-index/
      // embedded/sub-ms/Apache-2.0-self-host = no.
      durable: "unknown",
      provenance: "no",
      temporal: "yes",
      latency: "unknown",
      throughput: "unknown",
      embedded: "no",
      mcp: "unknown",
      x402: "no",
      license: "unknown",
    }),
    faqs: [
      {
        question: "Is Zep better than AllSource?",
        answer:
          "Zep is a temporal memory service focused on conversational history; AllSource is a general-purpose durable event store where temporal queries apply to all your data. Zep fits managed chat memory. AllSource fits full event provenance, embedded mode, and Apache-2.0 Core self-hosting. Its published 11.9μs p99 result covers Core indexed reads, not end-to-end memory recall.",
      },
      {
        question: "Does Zep support temporal queries like AllSource?",
        answer:
          "Zep offers temporal memory for chat history. AllSource makes temporal a first-class property of the entire event log — any past state of any entity is reconstructable via as_of projections.",
      },
      {
        question: "Can I self-host AllSource instead of using a hosted memory service?",
        answer:
          "Yes. AllSource is Apache-2.0 licensed, runs embedded in-process or as a server, and has hosted tiers if you prefer managed. You are never locked into a single hosted memory API.",
      },
    ],
    sources: [
      { label: "Zep documentation", href: "https://help.getzep.com/" },
      { label: "Zep repository", href: "https://github.com/getzep/zep" },
    ],
  },

  // stoolap is a DIFFERENT CATEGORY from the three above: it is an embedded,
  // in-process relational SQL database (mutable tables, JOINs, optimizer), not
  // an agent-memory layer. Unlike mem0/letta/zep it HAS an MCP server, native
  // vector/HNSW + built-in embeddings, and time-travel — so we must NOT claim
  // any of those as AllSource-exclusive wins. Rows below are authored directly
  // (not via baseRows) and grounded in docs/research/stoolap-vs-allsource.md.
  stoolap: {
    slug: "stoolap",
    name: "stoolap",
    verdict:
      "stoolap is an embedded relational SQL database in pure Rust — mutable tables, JOINs, an optimizer, all in your process. AllSource is an immutable, replayable event store and agent-memory service you run over the network or host. They overlap on AI features (vectors, embeddings, time-travel, an MCP server) but solve different problems — pick by data model and deployment, not by a benchmark.",
    metaDescription:
      "AllSource vs stoolap: an honest, sourced comparison of an immutable event store + agent-memory service against an embedded SQL database. Both are Rust, Apache-2.0, with vectors, embeddings, time-travel, and an MCP server — the real difference is mutable SQL tables vs an immutable event log, and embedded library vs network service. In-memory speed is a near-wash; reproduce it yourself.",
    category: {
      heading: "First, the honest part: these are different categories",
      body: "stoolap is an embedded relational SQL DBMS (SQLite/DuckDB-class) you compile into one app: CREATE TABLE, INSERT, SELECT, mutable rows, JOINs, a cost-based optimizer — with no server mode. AllSource is an append-only event store plus an agent-memory engine (Prime) and a hosted multi-tenant SaaS, served over the network or self-hosted. Both are pure Rust, Apache-2.0, and both ship vectors + HNSW, built-in embeddings, time-travel, and an MCP server — so this is not a feature-checkbox contest. The decision is the data model (mutable SQL tables vs an immutable, replayable log) and the deployment (a zero-ops embedded library vs a shared network service with multi-tenancy and per-call billing).",
    },
    selfHostLabel: "Self-host (free, Apache-2.0)",
    tableNote:
      "Both are Rust and Apache-2.0 (AllSource's enterprise tier is BSL 1.1). stoolap genuinely ships an MCP server (30 SQL tools), native VECTOR/HNSW search, a built-in EMBED() with no external API, and AS OF time-travel — none of those are AllSource-exclusive here. On the only fair head-to-head (single-row in-memory ingest + point reads) the two land in the same order of magnitude; reproduce it with `cargo run --release -p allsource-performance`. stoolap multipliers like 191×/1213× are vendor-reported; an independent Better Stack test measured stoolap's real OLAP edge over SQLite at ~4–6×.",
    articleSlug: "allsource-vs-stoolap",
    sources: [
      { label: "stoolap repository", href: "https://github.com/stoolap/stoolap" },
      { label: "AllSource reproduction notes", href: "/blog/allsource-vs-stoolap" },
    ],
    pickAllsource: [
      "You need an immutable, replayable store of record — full event provenance and any past state by replay, as an audit ledger (not a garbage-collected MVCC version)",
      "The store must be a shared network service for many clients/agents — a Rust server on :3900 with leader-follower replication — not a library trapped in one process",
      "You want a hosted, multi-tenant SaaS with RBAC, billing, and x402 per-call agent payments out of the box",
      "You want a purpose-built agent-memory engine (Prime: knowledge graph + vector + recall) rather than assembling memory yourself over SQL",
    ],
    pickCompetitor: [
      "You want real SQL — JOINs, CTEs, window functions, a cost-based optimizer — over mutable tables, with no projections to fold",
      "You want a database embedded in one app with zero ops: no server, no Docker, no network hop — `cargo add stoolap` (or npm/pip) and ship",
      "You want fast local OLAP (GROUP BY, DISTINCT, aggregations) without standing up DuckDB",
      "You want vector search + built-in embeddings inside SQL (VECTOR(N), HNSW, EMBED()), broad language reach, and fully-permissive Apache-2.0 with no BSL strings on any feature",
    ],
    rows: [
      {
        feature: "Category",
        allsource: "Event store + agent memory",
        competitor: "Embedded SQL DBMS",
        note: "AllSource is an append-only event store, the Prime memory engine, and a hosted SaaS. stoolap is a SQLite/DuckDB-class relational database you compile into one app.",
      },
      {
        feature: "Core data model",
        allsource: "Immutable event log",
        competitor: "Mutable SQL tables",
        note: "The decision that drives everything else: keep an append-only, replayable history and project state from it (AllSource), or run SQL and UPDATE/DELETE rows in place (stoolap).",
      },
      {
        feature: "Query language",
        allsource: "HTTP/MCP + EventQL (SQL analytics)",
        competitor: "Full SQL (JOINs, CTEs, windows)",
        note: "stoolap is a SQL DBMS front-and-center with a cost-based optimizer. If you want SQL ergonomics over your data, stoolap wins outright.",
      },
      {
        feature: "Written in Rust",
        allsource: "yes",
        competitor: "yes",
        note: "Both are pure Rust. stoolap is 99.8% Rust (it migrated from an early Go prototype); AllSource Core is Rust with Go/Elixir services around it.",
      },
      {
        feature: "Embeddable in-process",
        allsource: "yes",
        competitor: "yes (its only mode)",
        note: "AllSource can embed via `allsource-core` but is built to run as a server. stoolap is embedded-only — there is no standalone server or daemon.",
      },
      {
        feature: "Network service / shared backbone",
        allsource: "yes (Rust server :3900)",
        competitor: "no",
        note: "You cannot point a fleet of clients at a stoolap instance over the network; it lives in one process. AllSource is designed to be a shared service.",
      },
      {
        feature: "Durable (survives restart)",
        allsource: "WAL + Parquet",
        competitor: "WAL + columnar files",
        note: "Both are durable. stoolap pairs an in-memory MVCC hot buffer + WAL with immutable cold columnar files; AllSource is WAL (CRC32, fsync) + Parquet (Snappy).",
      },
      {
        feature: "In-memory ingestion (single-thread, 100k)",
        allsource: "~200K–506K events/sec",
        competitor: "~291K–448K rows/sec",
        note: "Measured on the same M2 Max laptop, --release. A near-wash — same order of magnitude, ranges overlap with background load. Nobody is 10× faster. (events/sec ≠ rows/sec, but comparable per-record cost.)",
      },
      {
        feature: "In-memory point reads",
        allsource: "11.9μs p99 (published)",
        competitor: "~0.5–0.9μs avg (measured)",
        note: "Different metrics (p99 vs avg) on different harnesses — do NOT read this as a 20× win. Honest statement: both do point reads in the single-digit-microsecond-or-better range in memory.",
      },
      {
        feature: "Durable single-row ingest (fsync)",
        allsource: "not isolated here",
        competitor: "~79K–97K rows/sec",
        note: "Neither headline number is a pure synchronous-durable single-row rate. The only durable single-row figure measured is stoolap's, with fsync on the WAL.",
      },
      {
        feature: "Vector / HNSW search",
        allsource: "yes (fastembed + HNSW)",
        competitor: "yes (VECTOR(N), HNSW)",
        note: "Both ship native vector search with an HNSW index. This is NOT an AllSource-exclusive axis against stoolap.",
      },
      {
        feature: "Built-in embeddings (no external API)",
        allsource: "yes (local fastembed)",
        competitor: "yes (built-in EMBED())",
        note: "Both compute embeddings locally with no Python and no external embedding service.",
      },
      {
        feature: "Temporal / time-travel",
        allsource: "as_of projections (replay log)",
        competitor: "AS OF TIMESTAMP/TRANSACTION (MVCC)",
        note: "Both offer time-travel. The difference is what it reads: AllSource replays a permanent immutable log; stoolap reads retained MVCC versions (a concurrency mechanism, garbage-collected — not a permanent audit ledger).",
      },
      {
        feature: "Full event provenance / replay",
        allsource: "yes (first-class)",
        competitor: "no",
        note: "AllSource keeps the full immutable history by design; every change is a replayable event. stoolap's MVCC versions are not an audit trail.",
      },
      {
        feature: "MCP server for AI agents",
        allsource: "55 tenant / 73 fleet tools",
        competitor: "30 SQL tools",
        note: "Both ship a first-party MCP server. stoolap's 30 tools are a SQL surface (query, execute, transactions, schema, vacuum); AllSource's are event-store/agent-memory verbs (ingest, recall, projections, anomaly detection).",
      },
      {
        feature: "Agent-memory product",
        allsource: "Prime (graph + vector + recall)",
        competitor: "no",
        note: "AllSource ships a dedicated memory engine with its own MCP tools. stoolap gives you a SQL + vector store; assembling agent memory on top is your job.",
      },
      {
        feature: "Multi-tenancy / RBAC / billing",
        allsource: "yes (Control Plane)",
        competitor: "no",
        note: "Tenants, RBAC, LemonSqueezy billing, and x402 are built in for AllSource. For a library like stoolap, tenancy and billing are your app's job.",
      },
      {
        feature: "x402 per-call agent payments",
        allsource: "yes",
        competitor: "no",
        note: "AllSource ships native x402 micropayments so agents pay per call. stoolap has nothing to price — there is no hosted offering.",
      },
      {
        feature: "License (OSS core)",
        allsource: "Apache-2.0 (enterprise BSL 1.1)",
        competitor: "Apache-2.0",
        note: "Both are permissive OSS you can self-host for free. stoolap is fully Apache-2.0 with no commercial strings; AllSource's enterprise features are BSL 1.1 (converting to Apache-2.0 in 2029).",
      },
      {
        feature: "Maturity",
        allsource: "v0.22, org-maintained",
        competitor: "v0.4.0, ~1.2k★, 1 author",
        note: "stoolap is a young single-author project (latest v0.4.0); an independent review notes it is 'still in its early days.' Both are pre-1.0.",
      },
    ],
    faqs: [
      {
        question: "Is stoolap better than AllSource?",
        answer:
          "They are different categories, so 'better' depends on the job. stoolap is an embedded relational SQL database — mutable tables, JOINs, a cost-based optimizer, fast local OLAP, all in your process with zero ops. AllSource is an immutable, replayable event store plus an agent-memory engine (Prime) and a hosted multi-tenant SaaS served over the network. If you want SQL over mutable data in one app, stoolap is simpler and there is nothing to operate. If you want a replayable store of record with full provenance, served to a fleet of agents with per-call billing, that is AllSource's shape. On raw in-memory speed they are close.",
      },
      {
        question: "Does stoolap have an MCP server, vector search, and time-travel like AllSource?",
        answer:
          "Yes — all three. Unlike a typical agent-memory tool, stoolap ships a first-party MCP server (30 SQL tools for Claude Desktop, Claude Code, Cursor, etc.), native VECTOR(N) columns with HNSW indexing and a built-in EMBED() function (no external embedding API), and AS OF time-travel via MVCC. So none of those are AllSource-exclusive in this comparison. The difference is what they do: stoolap's MCP exposes SQL; AllSource's exposes event-store and agent-memory verbs, and AllSource's time-travel replays a permanent immutable log rather than retained MVCC versions.",
      },
      {
        question: "Is AllSource faster than stoolap?",
        answer:
          "On the only fair head-to-head — single-row in-memory ingestion plus point reads, single-thread, same M2 Max laptop, release builds — it is a near-wash. AllSource's batch path edges ahead on a clean machine (~506K vs stoolap's ~291–448K rows/sec), but the ranges overlap and depend on background load; nobody is 10× faster. stoolap's published multipliers (191× vs DuckDB, 1213× vs DuckDB on SELECT) are vendor-reported and partly inflated — an independent Better Stack test found stoolap's real OLAP advantage over SQLite to be ~4–6×. Reproduce AllSource's number yourself with `cargo run --release -p allsource-performance`.",
      },
      {
        question: "Can I self-host both stoolap and AllSource?",
        answer:
          "Yes. stoolap is Apache-2.0 and embedded by design — `cargo add stoolap` (or its Node/Python/Go/Java/C#/PHP/Ruby/Swift/WASM bindings) and it runs in your process. AllSource community is Apache-2.0 too and runs embedded or as a server you self-host for free; its enterprise features are BSL 1.1, and it also offers hosted tiers. Both are permissive open source, so 'self-hostable' is true of both.",
      },
      {
        question: "Should I use stoolap or AllSource for agent memory?",
        answer:
          "If you want a turnkey agent-memory layer — durable recall, a knowledge graph, vector + recency re-ranking, and dedicated MCP tools — AllSource's Prime is purpose-built for it. stoolap gives you the raw materials (SQL, vectors, embeddings, time-travel) in one embedded engine, but you assemble the memory semantics yourself. Choose stoolap if you want full control inside one app and are happy to build the recall logic; choose AllSource/Prime if you want the memory engine and a shared, replayable backbone for a fleet of agents.",
      },
    ],
  },
};

export const competitorSlugs = Object.keys(competitors) as CompetitorSlug[];
