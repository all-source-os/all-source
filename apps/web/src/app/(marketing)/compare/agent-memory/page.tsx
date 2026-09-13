"use client";

import { Badge, buttonVariants, cn, Section } from "@allsource/ui";
import { ChevronRight, Minus, Plus } from "lucide-react";
import Link from "next/link";
import { staticMotion as motion } from "@/components/ui/static-motion";

// Five approaches to AI agent memory, ordered roughly by how "managed" each
// is — least infrastructure first, most infrastructure last. AllSource Prime
// is "event-sourced memory"; called out explicitly but the page is written
// to be useful even for readers who pick something else.
type Approach = {
  id: string;
  name: string;
  examples: string;
  blurb: string;
  wins: string[];
  loses: string[];
  // Set to render the "Try AllSource Prime →" CTA on this card.
  ours?: boolean;
};

const approaches: Approach[] = [
  {
    id: "platform",
    name: "Platform memory",
    examples: "Claude built-in memory · ChatGPT memory · Gemini saved info",
    blurb:
      "Memory features built into the model vendor's product surface. You enable a setting, the assistant starts remembering across conversations.",
    wins: [
      "Zero configuration — turn it on in settings, done",
      "Native UX integrated with the chat interface",
      "No code, no infrastructure",
    ],
    loses: [
      "Locked to one vendor — your memory in Claude isn't available in ChatGPT",
      "API and export paths vary by provider and product",
      "Product memory history is not the same as your application's decision history",
      "Memory shape is decided by the vendor, not you",
    ],
  },
  {
    id: "retrieval",
    name: "Retrieval / RAG memory",
    examples: "Mem0 · Zep · raw vector DBs (Pinecone, Weaviate, pgvector)",
    blurb:
      "Store conversation chunks (or extracted facts) as vector embeddings; retrieve via semantic similarity at query time. The dominant pattern for the past two years.",
    wins: [
      "Works for unstructured content — chat logs, documents, notes",
      "Multi-tool friendly via REST APIs",
      "Semantic recall finds adjacent ideas, not just exact matches",
    ],
    loses: [
      "Similarity is not truth — the top match can be plausible and wrong",
      "Hard to verify what's in the store without querying with the right phrasing",
      "No first-class graph — relationships between facts are not modeled",
      "Point-in-time reconstruction needs timestamps, versions, and retrieval rules you design",
    ],
  },
  {
    id: "files",
    name: "File-based memory",
    examples: "CLAUDE.md · AGENTS.md · project-local markdown / JSON",
    blurb:
      "Human-readable files in a folder. Agents read and edit them directly. Often committed to git so changes are reviewable.",
    wins: [
      "Trivial to inspect — open the file in any editor",
      "Version-controlled for free via git",
      "Zero infrastructure",
      "Works offline without a separate service",
    ],
    loses: [
      "Manual merge conflicts when two agents (or an agent and a human) edit the same file",
      'No structured queries — "all decisions involving Alice" requires grep',
      "No typed relations between facts",
      "Search, concurrent edits, and schema become your responsibility as the collection grows",
    ],
  },
  {
    id: "database",
    name: "Database memory",
    examples: "Postgres + CRUD · Supabase · Drizzle/Prisma + agent functions",
    blurb:
      "A relational table with rows for entities; the agent does CRUD via function-call tools. Leverages skills your team already has.",
    wins: [
      "Structured, queryable, durable",
      "Uses tooling your team already knows (migrations, ORMs, SQL)",
      "Constraints enforce schema at write time",
    ],
    loses: [
      "Every schema change is a migration",
      "Point-in-time business history needs a history model, not only current rows",
      "Relationships need schema and query design",
      "Semantic recall needs an embedding and indexing path",
    ],
  },
  {
    id: "event-sourced",
    name: "Event-sourced memory",
    examples: "AllSource Prime · Neotoma · roll-your-own event store",
    blurb:
      "Memory as an append-only log of events. Current state is projected from the log; full history is preserved. AllSource Prime adds a knowledge graph and vector recall on top of the same event spine.",
    wins: [
      "Replayable change history when complete events are captured",
      "Source-event provenance when writes record their evidence",
      "Graph + vector recall in one query (Prime's `prime_recall`)",
      "Hosted multi-tenant or local-first — same data shape both ways",
      "Cross-tool sync via MCP — same memory in Claude Desktop, the Anthropic CLI, Cursor, OpenCode",
    ],
    loses: [
      "More infrastructure than a markdown file or platform memory",
      "Conceptually different from CRUD (events, not rows) — learning curve",
      "Newer category — fewer drop-in tutorials than for Postgres or vector DBs",
    ],
    ours: true,
  },
];

const decisionMatrix = [
  {
    approach: "Platform memory",
    fit: "Personalization inside one assistant",
    limit: "Application decision history remains a separate design problem.",
    source: "OpenAI Memory FAQ",
    href: "https://help.openai.com/en/articles/8590148",
  },
  {
    approach: "Retrieval / RAG",
    fit: "Finding relevant passages in a document corpus",
    limit: "Answer quality depends on content preparation and retrieval configuration.",
    source: "Microsoft RAG guidance",
    href: "https://learn.microsoft.com/en-us/azure/foundry/concepts/retrieval-augmented-generation?view=foundry-classic",
  },
  {
    approach: "Files",
    fit: "Small, inspectable project instructions",
    limit: "Structured queries and concurrent updates need additional tooling.",
    source: "Claude Code memory docs",
    href: "https://code.claude.com/docs/en/memory",
  },
  {
    approach: "Database",
    fit: "Structured current state and familiar CRUD",
    limit: "Add application-level history when past decisions must be reconstructed.",
    source: "Microsoft event sourcing guidance",
    href: "https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing",
  },
  {
    approach: "Event-sourced",
    fit: "Replayable changes and inspectable provenance",
    limit: "Design event schemas and projections; semantic recall is another layer.",
    source: "Microsoft event sourcing guidance",
    href: "https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing",
  },
] as const;

function Card({ approach }: { approach: Approach }) {
  return (
    <div
      className={cn(
        "rounded-xl border bg-card p-6",
        approach.ours && "border-primary/50 bg-primary/5"
      )}
    >
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="text-xl font-semibold">{approach.name}</h3>
        {approach.ours && (
          <Badge variant="default" className="text-xs">
            AllSource Prime
          </Badge>
        )}
      </div>
      <div className="mb-3 font-mono text-xs text-muted-foreground">{approach.examples}</div>
      <p className="mb-5 text-sm text-muted-foreground">{approach.blurb}</p>

      <div className="grid gap-4 md:grid-cols-2">
        <div>
          <div className="mb-2 text-xs font-medium uppercase tracking-wide text-green-600 dark:text-green-400">
            When it wins
          </div>
          <ul className="space-y-1.5">
            {approach.wins.map((w) => (
              <li key={w} className="flex items-start gap-2 text-sm">
                <Plus className="mt-0.5 h-3.5 w-3.5 shrink-0 text-green-500" />
                <span>{w}</span>
              </li>
            ))}
          </ul>
        </div>
        <div>
          <div className="mb-2 text-xs font-medium uppercase tracking-wide text-muted-foreground">
            When it loses
          </div>
          <ul className="space-y-1.5">
            {approach.loses.map((l) => (
              <li key={l} className="flex items-start gap-2 text-sm">
                <Minus className="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-foreground/60" />
                <span>{l}</span>
              </li>
            ))}
          </ul>
        </div>
      </div>

      {approach.ours && (
        <div className="mt-5 flex flex-wrap gap-2">
          <Link href="/connect" className={cn(buttonVariants({ variant: "default" }), "gap-1.5")}>
            Try AllSource Prime <ChevronRight className="h-4 w-4" />
          </Link>
          <Link href="/prime" className={cn(buttonVariants({ variant: "ghost" }), "text-sm")}>
            Read about Prime
          </Link>
        </div>
      )}
    </div>
  );
}

export default function CompareAgentMemoryPage() {
  return (
    <div className="relative overflow-hidden">
      <Section className="relative pt-24 pb-12 text-center">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6 }}
        >
          <h1 className="text-4xl font-bold tracking-tight sm:text-5xl">
            Agent memory: five approaches, honestly compared
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-lg text-muted-foreground">
            There are five common approaches to AI agent memory: built-in platform memory,
            RAG/retrieval, file-based notes, a conventional database, and an event-sourced log.
            Every team picks one, usually by accident. This page sets out where each approach wins,
            where each breaks down, and how to choose deliberately.
          </p>
        </motion.div>
      </Section>

      <Section className="pb-12">
        <div className="mx-auto max-w-3xl">
          <h2 className="text-2xl font-semibold">Choose by evidence need</h2>
          <p className="mt-3 text-sm leading-6 text-muted-foreground">
            These are approach-level trade-offs, not guarantees about every product. Sources show
            representative implementations.
          </p>
          <div className="mt-6 overflow-hidden rounded-xl border">
            <table className="w-full table-fixed border-collapse text-left text-sm">
              <caption className="sr-only">
                Agent memory approaches, best fits, and limitations
              </caption>
              <thead className="bg-muted/40">
                <tr>
                  <th scope="col" className="w-1/3 px-4 py-3 font-semibold">
                    Approach and source
                  </th>
                  <th scope="col" className="px-4 py-3 font-semibold">
                    Best fit and limitation
                  </th>
                </tr>
              </thead>
              <tbody>
                {decisionMatrix.map(({ approach, fit, limit, source, href }) => (
                  <tr key={approach} className="border-t align-top">
                    <th scope="row" className="break-words px-4 py-4 font-medium">
                      {approach}
                      <a
                        href={href}
                        className="mt-2 block text-xs font-normal text-primary underline underline-offset-2"
                      >
                        {source}
                      </a>
                    </th>
                    <td className="px-4 py-4">
                      <p className="font-medium">{fit}</p>
                      <p className="mt-1 text-muted-foreground">{limit}</p>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </Section>

      <Section className="pb-16">
        <div className="mx-auto flex max-w-3xl flex-col gap-6">
          {approaches.map((a) => (
            <Card key={a.id} approach={a} />
          ))}
        </div>
      </Section>

      <Section className="pb-24">
        <div className="mx-auto max-w-3xl rounded-xl border bg-muted/20 p-6">
          <h2 className="text-xl font-semibold">Need to test restart durability?</h2>
          <p className="mt-3 text-sm leading-6 text-muted-foreground">
            The comparison is a decision aid. Run a local write, restart, recall, and history check
            before choosing a memory layer for production.
          </p>
          <div className="mt-4 flex flex-wrap gap-4 text-sm">
            <Link
              href="/agent-memory-restart-proof"
              className="font-medium text-primary underline underline-offset-2"
            >
              Run the restart proof
            </Link>
            <Link
              href="/solutions/agent-memory"
              className="font-medium text-primary underline underline-offset-2"
            >
              Explore the agent-memory solution
            </Link>
          </div>
        </div>
      </Section>
    </div>
  );
}
