import { Badge, buttonVariants, cn } from "@allsource/ui";
import { ArrowRight, CheckCircle2, Database, GitBranch, History } from "lucide-react";
import Link from "next/link";
import { eventSourcingPatterns } from "@/lib/event-sourcing-patterns";
import { breadcrumbSchema, eventSourcingPatternListSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "Event Sourcing Patterns for Production Systems",
  description:
    "Ten production event-sourcing patterns for streams, concurrency, projections, replay, schemas, temporal queries, tenancy, and durable consumers.",
  canonical: "/event-sourcing/patterns",
});

function JsonLd({ value }: { value: object }) {
  return (
    <script
      type="application/ld+json"
      // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD requires script content; '<' is escaped before insertion
      dangerouslySetInnerHTML={{ __html: JSON.stringify(value).replace(/</g, "\\u003c") }}
    />
  );
}

export default function EventSourcingPatternsPage() {
  return (
    <div className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 sm:py-24 lg:px-8">
      <JsonLd
        value={breadcrumbSchema([
          { name: "Home", path: "/" },
          { name: "Event sourcing patterns", path: "/event-sourcing/patterns" },
        ])}
      />
      <JsonLd value={eventSourcingPatternListSchema(eventSourcingPatterns)} />

      <header className="grid gap-10 border-b border-border pb-14 lg:grid-cols-[1.2fr_0.8fr] lg:items-end">
        <div>
          <Badge variant="outline" className="font-mono text-xs uppercase tracking-[0.18em]">
            Event store field guide
          </Badge>
          <h1 className="mt-6 max-w-4xl text-balance text-4xl font-semibold leading-tight tracking-tight text-foreground sm:text-6xl">
            Production event-sourcing patterns, implemented in a real event store.
          </h1>
          <p className="mt-6 max-w-3xl text-pretty text-lg leading-8 text-muted-foreground sm:text-xl">
            Ten focused guides for designing immutable streams, safe writes, rebuildable views,
            historical queries, and recoverable consumers with AllSource Core.
          </p>
        </div>
        <div className="border-l-2 border-primary pl-5">
          <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
            Category first
          </p>
          <p className="mt-3 leading-7 text-foreground">
            Core is purpose-built event store database. Query Service derives tenant-facing views.
            Prime builds agent memory from same durable history.
          </p>
        </div>
      </header>

      <section aria-label="Event-sourcing guide structure" className="py-16">
        <div className="grid gap-6 lg:grid-cols-3">
          {[
            {
              icon: Database,
              title: "Write correct history",
              body: "Set aggregate boundaries, stable stream identity, expected versions, and schema contracts before facts become permanent.",
            },
            {
              icon: GitBranch,
              title: "Build disposable views",
              body: "Fold immutable events into projections that can rebuild, migrate side by side, and recover from durable checkpoints.",
            },
            {
              icon: History,
              title: "Operate through time",
              body: "Replay safely, reconstruct point-in-time state, preserve tenant boundaries, and explain every derived answer from source events.",
            },
          ].map((item) => (
            <article key={item.title} className="border border-border bg-card p-6">
              <item.icon className="h-5 w-5 text-primary" aria-hidden="true" />
              <h2 className="mt-5 text-xl font-semibold text-foreground">{item.title}</h2>
              <p className="mt-3 text-sm leading-6 text-muted-foreground">{item.body}</p>
            </article>
          ))}
        </div>
      </section>

      <section aria-labelledby="pattern-map-heading" className="border-t border-border py-16">
        <div className="max-w-3xl">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">Ten patterns</p>
          <h2 id="pattern-map-heading" className="mt-3 text-3xl font-semibold text-foreground">
            Follow one technical question at a time
          </h2>
          <p className="mt-4 leading-7 text-muted-foreground">
            Each page gives direct answer, design choices, AllSource implementation, failure modes,
            production checklist, and related next steps. No generated city-and-keyword
            permutations.
          </p>
        </div>

        <ol className="mt-10 grid gap-px overflow-hidden border border-border bg-border md:grid-cols-2">
          {eventSourcingPatterns.map((pattern, index) => (
            <li key={pattern.slug} className="bg-background">
              <Link
                href={`/event-sourcing/patterns/${pattern.slug}`}
                className="group flex h-full gap-5 p-6 transition-colors hover:bg-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring sm:p-8"
              >
                <span className="font-mono text-sm text-primary">
                  {String(index + 1).padStart(2, "0")}
                </span>
                <span>
                  <span className="text-xl font-semibold text-foreground group-hover:text-primary">
                    {pattern.shortTitle}
                  </span>
                  <span className="mt-3 block text-sm leading-6 text-muted-foreground">
                    {pattern.directAnswer}
                  </span>
                  <span className="mt-5 inline-flex items-center gap-2 font-mono text-xs font-semibold text-primary">
                    Read pattern <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
                  </span>
                </span>
              </Link>
            </li>
          ))}
        </ol>
      </section>

      <section className="grid gap-8 border-t border-border py-16 lg:grid-cols-[1fr_auto] lg:items-center">
        <div>
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Put patterns to work
          </p>
          <h2 className="mt-3 text-3xl font-semibold text-foreground">
            One durable record. Many rebuildable uses.
          </h2>
          <ul className="mt-6 grid gap-3 text-sm text-muted-foreground sm:grid-cols-2">
            {[
              "CRC32-checked write-ahead log",
              "Parquet persistence and compact reads",
              "Point-in-time reconstruction and snapshots",
              "Durable consumers and schema governance",
            ].map((item) => (
              <li key={item} className="flex items-start gap-2">
                <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-primary" aria-hidden="true" />
                {item}
              </li>
            ))}
          </ul>
        </div>
        <div className="flex flex-col gap-3 sm:flex-row">
          <Link href="/signup" className={cn(buttonVariants({ variant: "default" }), "gap-2")}>
            Start 14-day trial <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </Link>
          <Link href="/docs/api" className={cn(buttonVariants({ variant: "outline" }))}>
            Read API docs
          </Link>
        </div>
      </section>
    </div>
  );
}
