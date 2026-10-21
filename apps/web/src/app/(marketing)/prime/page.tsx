import { Badge, buttonVariants, Card, CardContent, cn } from "@allsource/ui";
import { ArrowRight } from "lucide-react";
import Link from "next/link";
import { FaGithub } from "react-icons/fa";
import { ChangelogSignup } from "@/components/changelog-signup";
import { FadeIn } from "@/components/ui/fade-in";

const PRIME_VERSION = "0.22.0";

const claudeDesktopConfig = `{
  "mcpServers": {
    "prime": {
      "command": "allsource-prime",
      "args": [
        "--data-dir", "~/.prime/memory",
        "--auto-inject"
      ]
    }
  }
}`;

const claudeCodeConfig = `claude mcp add prime allsource-prime \\
  --data-dir ~/.prime/memory \\
  --auto-inject`;

// Cursor reads stdio MCP servers from ~/.cursor/mcp.json with the same
// mcpServers shape as Claude Desktop. Verified against
// https://cursor.com/docs/context/mcp on 2026-05-24.
const cursorConfig = `{
  "mcpServers": {
    "prime": {
      "command": "allsource-prime",
      "args": [
        "--data-dir", "~/.prime/memory",
        "--auto-inject"
      ]
    }
  }
}`;

// OpenCode uses a different envelope (top-level "mcp", "type": "local",
// command as an array). Verified against https://opencode.ai/docs/mcp-servers
// on 2026-05-24.
const openCodeConfig = `{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "prime": {
      "type": "local",
      "command": ["allsource-prime", "--data-dir", "~/.prime/memory", "--auto-inject"],
      "enabled": true
    }
  }
}`;

const tools = [
  {
    name: "prime_add_node",
    blurb: "Create a node — person, concept, project, decision, insight.",
  },
  {
    name: "prime_add_edge",
    blurb: "Connect two nodes with a directed relation (works_on, impacts, depends_on…).",
  },
  {
    name: "prime_embed",
    blurb:
      "Make a node findable by meaning. Pass text — Prime embeds it server-side via fastembed.",
  },
  {
    name: "prime_recall",
    blurb:
      "Hybrid recall: vector similarity + graph proximity + temporal recency. Pass text or a vector.",
  },
  {
    name: "prime_context",
    blurb: "Combined retrieval with tiered depth (L0 stats / L1 conversation / L2 full hybrid).",
  },
  {
    name: "prime_index",
    blurb:
      "Compressed knowledge index — token-efficient markdown of everything you know, by domain.",
  },
  {
    name: "prime_neighbors",
    blurb: "Walk the graph around a node. Multi-hop BFS with direction + relation filters.",
  },
  {
    name: "prime_search",
    blurb: "List all nodes of a given type. Cheap, broad.",
  },
  {
    name: "prime_shortest_path",
    blurb: "Find how two entities are connected — the chain of relationships between them.",
  },
  {
    name: "prime_similar",
    blurb: "Find the most semantically similar embeddings to a stored vector.",
  },
  {
    name: "prime_history",
    blurb: "Full audit trail for any entity — every creation, update, and deletion.",
  },
  {
    name: "prime_forget",
    blurb: "Soft-delete a node. Invisible to queries, preserved in history.",
  },
  {
    name: "prime_stats",
    blurb: "Graph overview: total nodes, edges, types, relations.",
  },
];

const fitGuidance = [
  {
    title: "Use Prime for agent memory",
    body: "Choose Prime when agents need graph and vector recall, compressed context, temporal history, and provenance through MCP.",
  },
  {
    title: "Use Core without Prime for event history",
    body: "Choose Core alone when replay, point-in-time reconstruction, and projections matter but semantic or graph recall does not.",
  },
  {
    title: "Use a CMS for human publishing workflows",
    body: "Choose a CMS when editorial UI, approvals, previews, scheduling, and collaborative page editing are primary requirements.",
  },
];

export default function PrimeLandingPage() {
  return (
    <div className="mx-auto w-full max-w-screen-md px-4 py-24 lg:px-8">
      {/* Hero */}
      <FadeIn delay={0.1} inView>
        <Badge variant="outline" className="mb-4 text-xs font-mono">
          allsource-prime v{PRIME_VERSION} · MCP
        </Badge>
        <h1 className="mb-3 text-3xl font-bold tracking-tight text-foreground sm:text-5xl">
          Persistent memory for Claude and other MCP clients
        </h1>
        <p className="text-lg text-muted-foreground">
          Prime stores knowledge-graph relationships, embeddings, provenance, and compressed context
          in a local event store. Claude Desktop, Claude Code, and other MCP clients read and write
          that memory directly.
        </p>
        <div className="mt-8 flex items-center gap-3">
          <Link href="#install" className={cn(buttonVariants({ variant: "default" }), "gap-1")}>
            Install Prime <ArrowRight className="h-4 w-4" />
          </Link>
          <Link href="/docs/prime/mcp" className={cn(buttonVariants({ variant: "outline" }))}>
            MCP setup docs
          </Link>
        </div>
      </FadeIn>

      {/* Install in 30s */}
      <FadeIn delay={0.2} inView>
        <section id="install" className="mt-16 scroll-mt-24">
          <h2 className="mb-2 text-2xl font-semibold text-foreground">
            One memory store across agent surfaces
          </h2>
          <p className="mb-6 text-sm text-muted-foreground">
            One <code className="rounded bg-muted px-1.5 py-0.5 font-mono">cargo install</code>, one
            config edit per client. The same Prime store serves Claude Desktop, Claude Code, Cursor,
            and OpenCode — one source of truth, every place your agents work.
          </p>

          <Card>
            <CardContent className="space-y-4 pt-6">
              <div>
                <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  1. Install the binary
                </div>
                <pre className="overflow-x-auto rounded-md border bg-muted/30 px-3 py-2 text-xs font-mono">
                  cargo install allsource-prime
                </pre>
              </div>

              <div>
                <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  2a. Claude Desktop —{" "}
                  <code className="font-mono">
                    ~/Library/Application Support/Claude/claude_desktop_config.json
                  </code>
                </div>
                <pre className="overflow-x-auto rounded-md border bg-muted/30 px-3 py-2 text-xs font-mono leading-relaxed">
                  {claudeDesktopConfig}
                </pre>
              </div>

              <div>
                <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  2b. Claude Code — one command
                </div>
                <pre className="overflow-x-auto rounded-md border bg-muted/30 px-3 py-2 text-xs font-mono leading-relaxed">
                  {claudeCodeConfig}
                </pre>
              </div>

              <div>
                <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  2c. Cursor — <code className="font-mono">~/.cursor/mcp.json</code>
                </div>
                <pre className="overflow-x-auto rounded-md border bg-muted/30 px-3 py-2 text-xs font-mono leading-relaxed">
                  {cursorConfig}
                </pre>
              </div>

              <div>
                <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
                  2d. OpenCode — <code className="font-mono">opencode.json</code>
                </div>
                <pre className="overflow-x-auto rounded-md border bg-muted/30 px-3 py-2 text-xs font-mono leading-relaxed">
                  {openCodeConfig}
                </pre>
              </div>

              <div className="rounded-md border border-dashed bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
                <strong className="text-foreground">Why this works.</strong> The{" "}
                <code className="font-mono">--auto-inject</code> flag exposes a{" "}
                <code className="font-mono">prime://auto-context</code> MCP resource. Each client
                reads it at conversation start — a compressed markdown index of everything
                you&apos;ve told it, organized by domain, in ~500–1,000 tokens. No prompt
                engineering required, and every client sees the same index because they share the
                same local Prime store (or, if you wire <code className="font-mono">--sync-to</code>
                , the same hosted one).
              </div>
              <div className="rounded-md border border-dashed bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
                <strong className="text-foreground">Local writer rule.</strong> Run one writer
                process against a local <code className="font-mono">--data-dir</code> at a time. Use
                separate directories or tenant-scoped hosted persistence for concurrent clients.
              </div>
            </CardContent>
          </Card>
        </section>
      </FadeIn>

      {/* What you get */}
      <FadeIn delay={0.25} inView>
        <section className="mt-16">
          <h2 className="mb-2 text-2xl font-semibold text-foreground">
            19 MCP tools Claude picks automatically
          </h2>
          <p className="mb-6 text-sm text-muted-foreground">
            You don&apos;t call these. Claude does — the tool descriptions are written for agent
            consumption. v{PRIME_VERSION} adds text-only inputs for{" "}
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono">prime_embed</code> and{" "}
            <code className="rounded bg-muted px-1.5 py-0.5 font-mono">prime_recall</code> so the
            model never has to hand over a precomputed vector.
          </p>
          <Card>
            <CardContent className="pt-6">
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b text-left text-xs uppercase tracking-wide text-muted-foreground">
                      <th className="px-3 py-2 font-medium">Tool</th>
                      <th className="px-3 py-2 font-medium">When Claude calls it</th>
                    </tr>
                  </thead>
                  <tbody>
                    {tools.map((tool) => (
                      <tr key={tool.name} className="border-b border-muted last:border-0">
                        <td className="whitespace-nowrap px-3 py-2 align-top font-mono text-xs">
                          {tool.name}
                        </td>
                        <td className="px-3 py-2 text-muted-foreground">{tool.blurb}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>
        </section>
      </FadeIn>

      {/* Product fit */}
      <FadeIn delay={0.3} inView>
        <section className="mt-16">
          <h2 className="mb-2 text-2xl font-semibold text-foreground">Where Prime fits</h2>
          <p className="mb-6 text-sm text-muted-foreground">
            Prime is a retrieval layer over durable event history, not a general database or a human
            publishing suite.
          </p>
          <div className="grid gap-4 md:grid-cols-3">
            {fitGuidance.map((item) => (
              <Card key={item.title}>
                <CardContent className="pt-6">
                  <h3 className="font-semibold text-foreground">{item.title}</h3>
                  <p className="mt-2 text-sm text-muted-foreground">{item.body}</p>
                </CardContent>
              </Card>
            ))}
          </div>
        </section>
      </FadeIn>

      {/* How it stores */}
      <FadeIn delay={0.35} inView>
        <section className="mt-16">
          <h2 className="mb-2 text-2xl font-semibold text-foreground">
            What&apos;s under the hood
          </h2>
          <p className="text-sm text-muted-foreground">
            Prime is an AllSource event store, not a separate database. Every mutation — node, edge,
            vector, soft-delete — appends an immutable event to the WAL and is replayed by
            projections that hold the queryable state. WAL + Parquet durability with CRC32
            checksums; Snappy-compressed columnar snapshots; in-memory Core indexed reads measured
            11.9µs p99 in the published reference benchmark. Hybrid recall adds graph and vector
            work and has separate end-to-end latency.
          </p>
          <p className="mt-3 text-sm text-muted-foreground">
            Embeddings are computed in-process via{" "}
            <Link
              href="https://crates.io/crates/fastembed"
              target="_blank"
              rel="noopener noreferrer"
              className="text-foreground underline-offset-4 hover:underline"
            >
              fastembed
            </Link>{" "}
            (AllMiniLML6V2, 384 dims, ~25 MB, auto-downloaded into the fastembed cache on first
            call). No external embedding service is required. Subsequent embeddings take ~1–3 ms on
            a modern CPU.
          </p>
          <div className="mt-6 flex flex-wrap items-center gap-3">
            <Link href="/platform/prime" className={cn(buttonVariants({ variant: "outline" }))}>
              Technical deep-dive
            </Link>
            <Link
              href="https://crates.io/crates/allsource-prime"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"
            >
              crates.io/allsource-prime ↗
            </Link>
            <Link
              href="https://github.com/all-source-os/all-source/tree/main/apps/prime-mcp"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 text-sm text-muted-foreground transition-colors hover:text-foreground"
            >
              <FaGithub className="h-3.5 w-3.5" /> Source
            </Link>
          </div>
        </section>
      </FadeIn>

      {/* CTA */}
      <FadeIn delay={0.4} inView>
        <Card className="mt-16 border-dashed">
          <CardContent className="pt-6">
            <h3 className="mb-2 font-semibold text-foreground">Already running Claude?</h3>
            <p className="mb-4 text-sm text-muted-foreground">
              Then you&apos;re halfway there. Install <code>allsource-prime</code>, add the MCP
              config block, restart your client. Claude starts writing to memory on its own — no
              prompt changes needed.
            </p>
            <Link href="#install" className={cn(buttonVariants({ variant: "default" }), "gap-1")}>
              Back to install <ArrowRight className="h-4 w-4" />
            </Link>
          </CardContent>
        </Card>
      </FadeIn>

      <ChangelogSignup placement="prime" className="mt-8 rounded-xl" />
    </div>
  );
}
