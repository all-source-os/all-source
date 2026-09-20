import { Badge, buttonVariants, cn } from "@allsource/ui";
import { ArrowRight, Database, GitBranch, Play, RotateCcw, Rows3 } from "lucide-react";
import Link from "next/link";
import { TrackedMarketingLink } from "@/components/tracked-marketing-link";
import { breadcrumbSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "What Is an Event Store Database? Uses and Trade-offs",
  description:
    "Learn how an event store database preserves immutable streams for replay, projections, provenance, and point-in-time state—and when simpler storage is better.",
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
    headline: "What is an event store database?",
    description:
      "A technical definition of event store databases, streams, projections, replay, provenance, and operational trade-offs.",
    mainEntityOfPage: "https://www.all-source.xyz/what-is-an-event-store",
    author: { "@type": "Person", name: "Decebal Dobrica", url: "https://decebaldobrica.com" },
    publisher: { "@id": "https://www.all-source.xyz/#organization" },
    dateModified: "2026-09-20",
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
          What is an event store database?
        </h1>
        <p className="mt-6 text-xl leading-9 text-foreground">
          An event store database records ordered, immutable facts about state changes. Instead of
          keeping only the latest value, it preserves the sequence that produced that value.
          Applications derive current state through projections, trace results back to source
          events, and replay history to reconstruct earlier state.
        </p>
      </header>

      <section aria-labelledby="event-sourcing-heading" className="py-14">
        <h2 id="event-sourcing-heading" className="text-3xl font-semibold text-foreground">
          Event sourcing is the model; the event store is the database
        </h2>
        <p className="mt-5 max-w-4xl text-lg leading-8 text-muted-foreground">
          Event sourcing models application state as a sequence of accepted events. An event store
          database supplies the storage rules that model needs: ordered streams, append-only writes,
          version checks, durable positions, replay, and historical reads. Current-state tables and
          caches remain useful read models, but they are derived views rather than the only
          surviving record.
        </p>
        <div className="mt-8 overflow-x-auto border border-border">
          <table className="w-full min-w-[42rem] border-collapse text-left">
            <caption className="sr-only">
              Differences between an event store database, a current-state database, and logs
            </caption>
            <thead className="bg-muted/60 text-sm text-foreground">
              <tr>
                <th className="px-5 py-4 font-semibold" scope="col">
                  System
                </th>
                <th className="px-5 py-4 font-semibold" scope="col">
                  Source of truth
                </th>
                <th className="px-5 py-4 font-semibold" scope="col">
                  Historical answer
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border text-sm text-muted-foreground">
              <tr>
                <th className="px-5 py-4 font-medium text-foreground" scope="row">
                  Event store database
                </th>
                <td className="px-5 py-4">Ordered domain events</td>
                <td className="px-5 py-4">Replay stream to sequence or timestamp</td>
              </tr>
              <tr>
                <th className="px-5 py-4 font-medium text-foreground" scope="row">
                  Current-state database
                </th>
                <td className="px-5 py-4">Latest stored values</td>
                <td className="px-5 py-4">Needs audit tables, snapshots, or backups</td>
              </tr>
              <tr>
                <th className="px-5 py-4 font-medium text-foreground" scope="row">
                  Application logs
                </th>
                <td className="px-5 py-4">Operational messages</td>
                <td className="px-5 py-4">Can explain execution but may not rebuild state</td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

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

      <section aria-labelledby="allsource-model-heading" className="py-14">
        <h2 id="allsource-model-heading" className="text-3xl font-semibold text-foreground">
          How AllSource maps the event-store model
        </h2>
        <p className="mt-5 max-w-4xl text-lg leading-8 text-muted-foreground">
          AllSource Core accepts events into immutable streams backed by a checksummed write-ahead
          log and Parquet persistence. Query Service derives tenant-facing HTTP, realtime,
          analytics, and projection reads. Prime builds provenance-aware agent memory from the same
          durable history. Event payloads remain in the event store; product analytics receives only
          fixed interaction names and aggregate-safe properties.
        </p>
        <div className="mt-8 grid gap-4 md:grid-cols-3">
          <Link
            href="/event-sourcing/patterns/aggregate-streams"
            className="border border-border bg-card p-6 transition-colors hover:border-primary/60"
          >
            <h3 className="font-semibold text-foreground">Model aggregate streams</h3>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">
              Scope history, preserve order, and use expected versions for concurrency.
            </p>
          </Link>
          <Link
            href="/event-sourcing/patterns/temporal-queries"
            className="border border-border bg-card p-6 transition-colors hover:border-primary/60"
          >
            <h3 className="font-semibold text-foreground">Reconstruct temporal state</h3>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">
              Ask what a stream contained at a sequence or point in time.
            </p>
          </Link>
          <Link
            href="/event-sourcing/patterns/projections-read-models"
            className="border border-border bg-card p-6 transition-colors hover:border-primary/60"
          >
            <h3 className="font-semibold text-foreground">Build projections</h3>
            <p className="mt-2 text-sm leading-6 text-muted-foreground">
              Fold durable events into purpose-built read models and rebuild them safely.
            </p>
          </Link>
        </div>
      </section>

      <footer className="flex flex-col gap-4 border-t border-border py-14 sm:flex-row sm:flex-wrap">
        <TrackedMarketingLink
          href="/docs/api"
          destination="api_docs"
          placement="event_store_definition_footer"
          className={cn(buttonVariants(), "min-h-12")}
        >
          Read API docs <ArrowRight className="ml-2 size-4" aria-hidden="true" />
        </TrackedMarketingLink>
        <TrackedMarketingLink
          href="/examples"
          destination="live_demo"
          placement="event_store_definition_footer"
          className={cn(buttonVariants({ variant: "outline" }), "min-h-12")}
        >
          Open live demo <Play className="ml-2 size-4" aria-hidden="true" />
        </TrackedMarketingLink>
        <TrackedMarketingLink
          href="/signup"
          destination="signup"
          placement="event_store_definition_footer"
          className={cn(buttonVariants({ variant: "outline" }), "min-h-12")}
        >
          Start 14-day trial
        </TrackedMarketingLink>
      </footer>
    </article>
  );
}
