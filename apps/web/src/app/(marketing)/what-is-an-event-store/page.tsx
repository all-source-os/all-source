import { Badge, buttonVariants, cn } from "@allsource/ui";
import { ArrowRight, Database, GitBranch, RotateCcw, Rows3 } from "lucide-react";
import Link from "next/link";
import { breadcrumbSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "What Is an Event Store? Definition, Uses, and Trade-offs",
  description:
    "An event store records ordered facts instead of overwriting current state. Learn streams, projections, replay, audit history, and when a simpler database is enough.",
  canonical: "/what-is-an-event-store",
});

const concepts = [
  {
    icon: GitBranch,
    title: "Events are facts",
    body: "Each accepted event records what changed, when it changed, and which stream it belongs to. Corrections append another event; they do not edit history in place.",
  },
  {
    icon: Rows3,
    title: "Streams preserve order",
    body: "Related events form a stream, commonly scoped to one entity or process. Stream versions support optimistic concurrency and deterministic replay.",
  },
  {
    icon: Database,
    title: "Projections answer reads",
    body: "A projection folds events into a read model: account balance, order status, agent memory, dashboard totals, or any other current-state view.",
  },
  {
    icon: RotateCcw,
    title: "Replay reconstructs state",
    body: "Because source events remain available, systems can rebuild a projection, inspect a past point in time, or test corrected logic against recorded history.",
  },
] as const;

export default function WhatIsAnEventStorePage() {
  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "What is an event store?", path: "/what-is-an-event-store" },
  ]);
  const article = {
    "@context": "https://schema.org",
    "@type": "TechArticle",
    headline: "What is an event store?",
    description:
      "A technical definition of event stores, streams, projections, replay, and operational trade-offs.",
    mainEntityOfPage: "https://www.all-source.xyz/what-is-an-event-store",
    author: { "@type": "Person", name: "Decebal Dobrica", url: "https://decebaldobrica.com" },
    publisher: { "@id": "https://www.all-source.xyz/#organization" },
    dateModified: "2026-08-27",
  };

  return (
    <article className="mx-auto w-full max-w-6xl px-4 py-20 sm:px-6 lg:px-8">
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: Static schema is JSON-serialized and escapes HTML delimiters.
        dangerouslySetInnerHTML={{ __html: JSON.stringify(breadcrumb).replace(/</g, "\\u003c") }}
      />
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: Static schema is JSON-serialized and escapes HTML delimiters.
        dangerouslySetInnerHTML={{ __html: JSON.stringify(article).replace(/</g, "\\u003c") }}
      />

      <header className="max-w-4xl border-b border-border pb-12">
        <Badge variant="outline" className="font-mono text-xs uppercase tracking-[0.18em]">
          Event sourcing fundamentals
        </Badge>
        <h1 className="mt-6 text-balance text-4xl font-semibold tracking-tight text-foreground sm:text-6xl">
          What is an event store?
        </h1>
        <p className="mt-6 text-xl leading-9 text-foreground">
          An event store is a database that records ordered, immutable facts about state changes.
          Instead of keeping only the latest value, it keeps the sequence that produced that value.
          Applications derive current state through projections and can replay the same history to
          rebuild or inspect earlier state.
        </p>
      </header>

      <section aria-labelledby="parts-heading" className="py-14">
        <h2 id="parts-heading" className="text-3xl font-semibold text-foreground">
          Four parts of the model
        </h2>
        <div className="mt-8 grid gap-px overflow-hidden border border-border bg-border md:grid-cols-2">
          {concepts.map(({ icon: Icon, title, body }) => (
            <section key={title} className="bg-card p-6 sm:p-8">
              <Icon className="size-6 text-primary" aria-hidden="true" />
              <h3 className="mt-4 text-xl font-semibold text-foreground">{title}</h3>
              <p className="mt-3 leading-7 text-muted-foreground">{body}</p>
            </section>
          ))}
        </div>
      </section>

      <section className="grid gap-8 border-y border-border py-14 lg:grid-cols-2">
        <div>
          <h2 className="text-2xl font-semibold text-foreground">Use one when history matters</h2>
          <ul className="mt-5 space-y-3 text-muted-foreground">
            <li>Audit and compliance need a traceable sequence of decisions.</li>
            <li>Projections must be rebuilt after logic changes.</li>
            <li>Operators need point-in-time answers or replay debugging.</li>
            <li>Agents must retain provenance across sessions.</li>
          </ul>
        </div>
        <div>
          <h2 className="text-2xl font-semibold text-foreground">Do not use one by default</h2>
          <p className="mt-5 leading-7 text-muted-foreground">
            A current-state database is usually simpler when overwriting rows is acceptable and you
            do not need replay, provenance, temporal queries, or multiple derived read models. Event
            sourcing adds modelling, versioning, and projection work; those costs need a real
            reason.
          </p>
        </div>
      </section>

      <footer className="flex flex-col gap-4 py-14 sm:flex-row">
        <Link href="/platform/event-sourcing" className={cn(buttonVariants(), "min-h-12")}>
          See AllSource Core <ArrowRight className="ml-2 size-4" aria-hidden="true" />
        </Link>
        <Link
          href="/event-sourcing/patterns"
          className={cn(buttonVariants({ variant: "outline" }), "min-h-12")}
        >
          Explore production patterns
        </Link>
      </footer>
    </article>
  );
}
