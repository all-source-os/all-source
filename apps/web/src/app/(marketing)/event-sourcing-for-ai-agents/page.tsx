import { buttonVariants, cn } from "@allsource/ui";
import { ChevronRight } from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";
import { BLOG_CATEGORIES, type BlogCategory, getBlogPosts, type Post } from "@/lib/blog";
import { breadcrumbSchema, faqPageSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

export const metadata: Metadata = constructMetadata({
  title: "Event Sourcing for AI Agents",
  description:
    "Direct answers about event sourcing for AI agents: restart-safe memory, provenance, time-travel queries, Core versus Prime, and deployment without PostgreSQL.",
  canonical: "/event-sourcing-for-ai-agents",
});

// Maps a post's free-form `category` frontmatter onto a known BlogCategory.
// Anything unrecognized falls into "engineering" so no post is ever dropped.
function normalizeCategory(raw?: string): BlogCategory {
  const value = (raw ?? "").replace(/['"]/g, "").trim().toLowerCase();
  if (value === "use-cases" || value === "product" || value === "engineering") {
    return value;
  }
  return "engineering";
}

const SECTION_BLURB: Record<BlogCategory, string> = {
  engineering: "How the durable event store is built — WAL, Parquet, recall, and the Rust core.",
  "use-cases": "Patterns for putting event-sourced memory to work in real agent systems.",
  product: "Product updates, releases, and the story behind AllSource and Prime.",
};

const COMPARISONS = [
  { slug: "mem0", name: "mem0" },
  { slug: "letta", name: "Letta" },
  { slug: "zep", name: "Zep" },
] as const;

const DIRECT_ANSWERS = [
  {
    question: "What is event-sourced agent memory?",
    answer:
      "Event-sourced agent memory stores each accepted observation, decision, and state change as an ordered event. Current memory is derived from that history instead of overwriting the only copy.",
  },
  {
    question: "Does AllSource memory survive a process restart?",
    answer:
      "AllSource Core recovers accepted event history from its write-ahead log and Parquet persistence after restart. Configurable fsync controls the durability and throughput trade-off.",
  },
  {
    question: "Does AllSource require PostgreSQL?",
    answer:
      "No. AllSource Core is the database: WAL and Parquet persist events and event-sourced system metadata, while concurrent in-memory indexes and service caches are rebuilt from Core. Current AllSource services require no PostgreSQL instance.",
  },
  {
    question: "Does AllSource replace a vector database?",
    answer:
      "Core supplies durable ordered history, replay, and provenance. AllSource Prime adds vector, graph, compressed-index, and temporal recall derived from Core events.",
  },
  {
    question: "When should I not use event-sourced agent memory?",
    answer:
      "Use simpler current-state storage when you do not need replay, provenance, audit history, or point-in-time reconstruction. Event sourcing adds event modelling and projection work.",
  },
] as const;

export default async function EventSourcingForAiAgentsPage() {
  const posts = await getBlogPosts();

  // Group posts by normalized category, newest first within each group.
  const grouped = new Map<BlogCategory, Post[]>();
  for (const post of posts) {
    const cat = normalizeCategory(post.category);
    const list = grouped.get(cat) ?? [];
    list.push(post);
    grouped.set(cat, list);
  }
  for (const list of grouped.values()) {
    list.sort((a, b) => (a.publishedAt < b.publishedAt ? 1 : -1));
  }

  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "Event Sourcing for AI Agents", path: "/event-sourcing-for-ai-agents" },
  ]);
  const faq = faqPageSchema(DIRECT_ANSWERS);

  return (
    <>
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
        dangerouslySetInnerHTML={{ __html: JSON.stringify(breadcrumb) }}
      />
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD structured data requires dangerouslySetInnerHTML
        dangerouslySetInnerHTML={{ __html: JSON.stringify(faq).replace(/</g, "\\u003c") }}
      />
      <div className="relative overflow-hidden">
        <div className="mx-auto w-full max-w-3xl px-4 py-16 sm:px-6 lg:px-8">
          {/* Hero */}
          <header className="mb-12">
            <p className="text-sm font-medium text-primary">The pillar guide</p>
            <h1 className="mt-2 text-4xl font-bold tracking-tight sm:text-5xl">
              Event Sourcing for AI Agents
            </h1>
            <p className="mt-6 text-lg text-muted-foreground">
              Agents forget. A chat window scrolls past, a process restarts, and the context is
              gone. Event sourcing fixes that at the storage layer: every accepted decision,
              observation, and message becomes an immutable event you can replay, inspect by
              timestamp, and use to rebuild current context.
            </p>
          </header>

          <section aria-labelledby="quick-answers" className="mb-12 rounded-xl border p-6">
            <h2 id="quick-answers" className="text-2xl font-bold">
              Quick answers
            </h2>
            <dl className="mt-6 space-y-5">
              {DIRECT_ANSWERS.map((item) => (
                <div key={item.question}>
                  <dt className="font-semibold">{item.question}</dt>
                  <dd className="mt-1 text-sm leading-6 text-muted-foreground">{item.answer}</dd>
                </div>
              ))}
            </dl>
          </section>

          {/* What it is */}
          <section className="prose dark:prose-invert max-w-none">
            <h2>What event sourcing gives an agent</h2>
            <p>
              Instead of overwriting state, you append events. The current state is a projection you
              derive from the log. AllSource Core recovers accepted event history after restarts
              from a Rust write-ahead log (CRC32 checksums, configurable fsync) and columnar
              Parquet. In-memory projections measured <strong>11.9μs p99</strong> reads in the
              published reference benchmark; the separate ingestion benchmark measured{" "}
              <strong>469K events/sec</strong>.
            </p>
            <ul>
              <li>
                <strong>Durable memory</strong> — accepted event history is recovered from WAL and
                Parquet after restart; configurable fsync sets the write durability policy.
              </li>
              <li>
                <strong>Full provenance</strong> — replay the log to reconstruct exactly what the
                agent knew and when.
              </li>
              <li>
                <strong>Time-travel queries</strong> — ask for any entity&rsquo;s state{" "}
                <em>as_of</em> a past moment, first-class.
              </li>
              <li>
                <strong>MCP access</strong> — 55 tenant-scoped tools by default, rising to 73 when
                fleet and administrative controls are enabled.
              </li>
            </ul>
          </section>

          {/* Comparisons */}
          <section className="mt-12">
            <h2 className="text-2xl font-bold">How AllSource compares</h2>
            <p className="mt-3 text-muted-foreground">
              Evaluating a managed memory layer? Here&rsquo;s how a durable event store stacks up
              against the popular agent-memory tools.
            </p>
            <div className="mt-6 grid gap-3 sm:grid-cols-3">
              {COMPARISONS.map((c) => (
                <Link
                  key={c.slug}
                  href={`/vs/${c.slug}`}
                  className="group rounded-xl border p-4 transition-colors hover:bg-muted/30"
                >
                  <span className="font-medium">AllSource vs {c.name}</span>
                  <ChevronRight className="ml-1 inline h-4 w-4 transition-transform group-hover:translate-x-1" />
                </Link>
              ))}
            </div>
          </section>

          {/* Deep dives — grouped by theme, pulled from the blog */}
          <section className="mt-12">
            <h2 className="text-2xl font-bold">Go deeper</h2>
            <p className="mt-3 text-muted-foreground">
              Every AllSource deep-dive, grouped by theme. New posts surface here automatically.
            </p>
            <div className="mt-6 space-y-10">
              {BLOG_CATEGORIES.map(({ value, label }) => {
                const list = grouped.get(value) ?? [];
                if (list.length === 0) return null;
                return (
                  <div key={value}>
                    <h3 className="text-lg font-semibold">{label}</h3>
                    <p className="mt-1 text-sm text-muted-foreground">{SECTION_BLURB[value]}</p>
                    <ul className="mt-4 space-y-3">
                      {list.map((post) => (
                        <li key={post.slug}>
                          <Link
                            href={`/blog/${post.slug}`}
                            className="group block rounded-lg border p-4 transition-colors hover:bg-muted/30"
                          >
                            <span className="font-medium group-hover:text-primary">
                              {post.title}
                            </span>
                            {post.summary && (
                              <span className="mt-1 block text-sm text-muted-foreground">
                                {post.summary}
                              </span>
                            )}
                          </Link>
                        </li>
                      ))}
                    </ul>
                  </div>
                );
              })}
            </div>
          </section>

          {/* CTA */}
          <section className="mt-16 rounded-xl border bg-muted/20 p-8 text-center">
            <h2 className="text-2xl font-bold">Test memory across a real process restart</h2>
            <p className="mt-4 text-sm text-muted-foreground">
              Run one local decision through write, recall, source history, restart, and recall
              again. No account or API key required.
            </p>
            <div className="mt-6 flex flex-wrap items-center justify-center gap-4">
              <Link
                href="/agent-memory-restart-proof"
                className={cn(buttonVariants({ variant: "default" }))}
              >
                Run restart proof <ChevronRight className="ml-1 h-4 w-4" />
              </Link>
              <Link href="/pricing" className={cn(buttonVariants({ variant: "outline" }))}>
                See hosted pricing
              </Link>
            </div>
          </section>
        </div>
      </div>
    </>
  );
}
