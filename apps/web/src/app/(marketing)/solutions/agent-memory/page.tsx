"use client";

import { buttonVariants, cn, Section } from "@allsource/ui";
import {
  Brain,
  Check,
  ChevronRight,
  Clock,
  Code2,
  Database,
  GitBranch,
  Layers,
  Network,
  Search,
  Shield,
  Terminal,
  Zap,
} from "lucide-react";
import Link from "next/link";
import { staticMotion as motion } from "@/components/ui/static-motion";
import { faqPageSchema } from "@/lib/structured-data";

const capabilities = [
  {
    question: "How does an agent remember?",
    answer:
      "Every accepted fact becomes an immutable event in AllSource Core. The WAL and Parquet files persist the event history; Prime derives graph nodes, vectors, and relationships from it for recall.",
    icon: Database,
    details: [
      "Graph nodes: entities, concepts, decisions",
      "Vector embeddings: semantic similarity search",
      "Compressed index: navigational scaffolding",
    ],
    metric: "469K events/sec",
    color: "from-blue-500/20 to-blue-500/5",
  },
  {
    question: "How fast is recall?",
    answer:
      "The published Core reference benchmark measured 11.9μs p99 projection reads. Prime hybrid recall adds vector similarity, graph traversal, and temporal recency, so end-to-end recall depends on query and hardware.",
    icon: Zap,
    details: [
      "Vector search: HNSW index over embeddings",
      "Graph expansion: 1-hop BFS from matches",
      "Compressed index: cross-domain reasoning",
    ],
    metric: "Core: 11.9μs p99",
    color: "from-yellow-500/20 to-yellow-500/5",
  },
  {
    question: "What about cross-domain questions?",
    answer:
      "Prime's compressed index adds domain summaries and cross-references to graph and vector retrieval. A published AllSource project benchmark measured its effect on one cross-domain evaluation; results depend on your corpus and queries.",
    icon: Network,
    details: [
      "Auto-generated from graph events",
      "Organized by domain with cross-references",
      "Evaluation method and corpus are published",
    ],
    metric: "Published evaluation",
    color: "from-purple-500/20 to-purple-500/5",
  },
  {
    question: "Can I time-travel?",
    answer:
      "Every mutation is an append-only event. Query any entity's state at any past timestamp. See who added what, when, and why.",
    icon: Clock,
    details: [
      "as_of queries: reconstruct past state",
      "Full audit trail: every create, update, delete",
      "Graph diff: what changed between two timestamps",
    ],
    metric: "Full provenance",
    color: "from-green-500/20 to-green-500/5",
  },
];

const productLayers = [
  {
    name: "AllSource Core",
    answer: "Durable ordered event history, replay, point-in-time reconstruction, and provenance.",
    href: "/platform/event-sourcing",
  },
  {
    name: "AllSource Prime",
    answer: "Graph, vector, compressed-index, and temporal recall derived from Core events.",
    href: "/docs/prime",
  },
  {
    name: "AllSource hosted",
    answer: "Managed Core access with retention, event, stream, and MCP limits set by plan.",
    href: "/pricing",
  },
];

const useCases = [
  {
    title: "Personal AI Assistant",
    description:
      "Claude remembers your project context across sessions. Yesterday's decisions inform today's answers.",
    icon: Brain,
  },
  {
    title: "Multi-Agent Knowledge Sharing",
    description:
      "Three agents work on different parts of a codebase. Findings flow through the shared graph.",
    icon: GitBranch,
  },
  {
    title: "Incident Response Memory",
    description:
      "Your oncall agent remembers every past incident. 'What happened last time this alert fired?'",
    icon: Shield,
  },
  {
    title: "Research Assistant",
    description:
      "Read 50 papers, build a knowledge graph. The compressed index surfaces unexpected cross-domain connections.",
    icon: Search,
  },
  {
    title: "Code Review Context",
    description:
      "Agent remembers past review feedback. 'Last time you said X about error handling in this module.'",
    icon: Code2,
  },
  {
    title: "Audit & Compliance",
    description:
      "Full provenance on every memory. Who added what, when, from what source. Time-travel to any past state.",
    icon: Layers,
  },
];

export default function AgentMemoryPage() {
  // Built from the SAME `capabilities` array rendered below, so the schema can
  // never claim an answer the page does not show. These are already written as
  // self-contained question/answer pairs — exactly the shape an answer engine
  // lifts — they were just not machine-readable until now.
  const faqJsonLd = faqPageSchema(
    capabilities.map((cap) => ({ question: cap.question, answer: cap.answer }))
  );

  return (
    <>
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
        dangerouslySetInnerHTML={{ __html: JSON.stringify(faqJsonLd) }}
      />
      <div className="relative overflow-hidden">
        {/* Hero */}
        <Section className="relative pt-24 pb-16 text-center">
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.6 }}
          >
            <h1 className="text-4xl font-bold tracking-tight sm:text-6xl">
              Durable graph and vector memory for AI agents
            </h1>
            <p className="mx-auto mt-6 max-w-2xl text-lg text-muted-foreground">
              Store graph relationships, embeddings, compressed context, and provenance in one local
              binary. Connect through 19 <code>prime_*</code> memory tools or the HTTP API; optional
              inbox and hound modules bring the full Prime registry to 27 tools.
            </p>
            <div className="mt-8 flex flex-col items-stretch justify-center gap-3 sm:flex-row sm:items-center">
              <div className="rounded-lg border bg-muted/50 px-4 py-2 font-mono text-sm">
                cargo install allsource-prime
              </div>
              <Link
                href="https://github.com/all-source-os/all-source"
                className={cn(buttonVariants({ variant: "outline" }))}
              >
                View on GitHub <ChevronRight className="ml-1 h-4 w-4" />
              </Link>
            </div>
          </motion.div>
        </Section>

        {/* Claude Desktop Config */}
        <Section className="pb-16">
          <div className="mx-auto max-w-2xl">
            <h2 className="mb-4 text-center text-2xl font-semibold">
              Connect Prime to Claude Desktop
            </h2>
            <div className="rounded-lg border bg-muted/30 p-6">
              <p className="mb-3 text-sm text-muted-foreground">
                Add to <code>~/.claude/claude_desktop_config.json</code>:
              </p>
              <pre className="overflow-x-auto rounded bg-black/80 p-4 text-sm text-green-400">
                {`{
  "mcpServers": {
    "prime": {
      "command": "allsource-prime",
      "args": ["--data-dir", "~/.prime/memory"]
    }
  }
}`}
              </pre>
              <p className="mt-3 text-sm text-muted-foreground">
                19 <code>prime_*</code> memory tools cover graph CRUD, vector search, hybrid recall,
                compressed index, and temporal queries.
              </p>
            </div>
          </div>
        </Section>

        {/* Capabilities */}
        <Section className="pb-16">
          <h2 className="mb-12 text-center text-3xl font-bold">How it works</h2>
          <div className="grid gap-8 md:grid-cols-2">
            {capabilities.map((cap, i) => (
              <motion.div
                key={cap.question}
                initial={{ opacity: 0, y: 20 }}
                whileInView={{ opacity: 1, y: 0 }}
                transition={{ delay: i * 0.1 }}
                viewport={{ once: true }}
                className="rounded-xl border p-6"
              >
                <div className="mb-4 flex items-center gap-3">
                  <div
                    className={cn(
                      "flex h-10 w-10 items-center justify-center rounded-lg bg-gradient-to-br",
                      cap.color
                    )}
                  >
                    <cap.icon className="h-5 w-5" />
                  </div>
                  <span className="rounded-full bg-muted px-3 py-1 text-xs font-medium">
                    {cap.metric}
                  </span>
                </div>
                <h3 className="mb-2 text-lg font-semibold">{cap.question}</h3>
                <p className="mb-4 text-sm text-muted-foreground">{cap.answer}</p>
                <ul className="space-y-1">
                  {cap.details.map((d) => (
                    <li key={d} className="flex items-start gap-2 text-sm">
                      <Check className="mt-0.5 h-4 w-4 shrink-0 text-green-500" />
                      {d}
                    </li>
                  ))}
                </ul>
              </motion.div>
            ))}
          </div>
          <p className="mt-6 text-center text-sm text-muted-foreground">
            Cross-domain result: see the{" "}
            <Link href="/blog/compressed-index-doubles-cross-domain-recall" className="underline">
              published project benchmark and method
            </Link>
            .
          </p>
        </Section>

        {/* Product fit */}
        <Section className="pb-16">
          <h2 className="mb-4 text-center text-3xl font-bold">Choose the layer you need</h2>
          <p className="mx-auto mb-8 max-w-2xl text-center text-muted-foreground">
            Core is the durable record. Prime adds retrieval. Hosted removes infrastructure work.
          </p>
          <div className="grid gap-4 md:grid-cols-3">
            {productLayers.map((layer) => (
              <Link
                key={layer.name}
                href={layer.href}
                className="rounded-xl border p-6 transition-colors hover:bg-muted/30"
              >
                <h3 className="font-semibold">{layer.name}</h3>
                <p className="mt-2 text-sm text-muted-foreground">{layer.answer}</p>
                <span className="mt-4 inline-flex items-center text-sm font-medium text-primary">
                  See details <ChevronRight className="ml-1 h-4 w-4" />
                </span>
              </Link>
            ))}
          </div>
          <p className="mx-auto mt-6 max-w-2xl text-center text-sm text-muted-foreground">
            Not every app needs event-sourced memory. Use simpler current-state storage when replay,
            provenance, and historical reconstruction do not affect your product or operations.
          </p>
        </Section>

        {/* Use Cases */}
        <Section className="pb-16">
          <h2 className="mb-12 text-center text-3xl font-bold">Use cases</h2>
          <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
            {useCases.map((uc, i) => (
              <motion.div
                key={uc.title}
                initial={{ opacity: 0, y: 20 }}
                whileInView={{ opacity: 1, y: 0 }}
                transition={{ delay: i * 0.08 }}
                viewport={{ once: true }}
                className="rounded-xl border p-6"
              >
                <uc.icon className="mb-3 h-8 w-8 text-purple-400" />
                <h3 className="mb-2 font-semibold">{uc.title}</h3>
                <p className="text-sm text-muted-foreground">{uc.description}</p>
              </motion.div>
            ))}
          </div>
        </Section>

        {/* Architecture */}
        <Section className="pb-16">
          <h2 className="mb-8 text-center text-3xl font-bold">One engine, not three databases</h2>
          <div className="mx-auto max-w-2xl">
            <pre className="overflow-x-auto rounded-lg border bg-muted/30 p-6 text-xs leading-relaxed">
              {`┌─────────────────────────────────────────────┐
│              AllSource Prime                 │
│                                              │
│  Graph    Vectors    Temporal    Compressed   │
│  Nodes    HNSW       History    Index        │
│  Edges    Embed      Time-travel Cross-refs  │
│           Similar    Diff                    │
│                                              │
│  ┌──────────────────────────────────────┐   │
│  │         AllSource Core Engine         │   │
│  │  WAL + Parquet + DashMap + HLC + CRDT │   │
│  │  469K events/sec │ 11.9μs p99 reads   │   │
│  └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘`}
            </pre>
            <p className="mt-4 text-center text-sm text-muted-foreground">
              Prime records vectors, graph nodes, and edges as events in Core, then derives
              queryable memory views from the same durable history.
            </p>
          </div>
        </Section>

        {/* CTA */}
        <Section className="pb-24 text-center">
          <h2 className="mb-4 text-3xl font-bold">Run the restart proof</h2>
          <p className="mb-8 text-muted-foreground">
            Write one decision, restart Prime, recall it, and inspect its source event. Local only;
            no account or API key.
          </p>
          <div className="flex flex-col items-stretch justify-center gap-3 sm:flex-row sm:items-center">
            <Link
              href="/agent-memory-restart-proof"
              className={cn(buttonVariants({ size: "lg" }), "gap-2")}
            >
              <Terminal className="h-4 w-4" />
              Run restart proof
              <ChevronRight className="h-4 w-4" />
            </Link>
          </div>
          <div className="mt-6 flex items-center justify-center gap-6 text-sm text-muted-foreground">
            <Link href="/docs" className="underline">
              Documentation
            </Link>
            <Link
              href="https://github.com/all-source-os/all-source/tree/main/apps/core/examples"
              className="underline"
            >
              Examples
            </Link>
            <Link href="https://github.com/all-source-os/all-source" className="underline">
              GitHub
            </Link>
          </div>
        </Section>
      </div>
    </>
  );
}
