import { Badge, buttonVariants, cn } from "@allsource/ui";
import { ArrowRight, CheckCircle2, CircleDot, Radio, RefreshCcw } from "lucide-react";
import Link from "next/link";
import { breadcrumbSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "AllSource Projections: Rebuildable Read Models from Event History",
  description:
    "See how tenant-scoped AllSource projections enable, backfill, atomically switch generations, update live, and rebuild from immutable Core events.",
  canonical: "/platform/projections",
});

const stages = [
  ["Enable", "Choose a supported projection template for one tenant."],
  ["Backfill", "Fold immutable Core events into a shadow generation."],
  ["Switch", "Make completed generation active without exposing partial state."],
  ["Follow", "Apply new accepted events and broadcast state updates."],
  ["Rebuild", "Repeat from history after logic changes or recovery."],
] as const;

export default function ProjectionsPage() {
  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "Query Service", path: "/platform/query-service" },
    { name: "Projections", path: "/platform/projections" },
  ]);

  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-20 sm:px-6 lg:px-8">
      <script
        type="application/ld+json"
        // biome-ignore lint/security/noDangerouslySetInnerHtml: Static schema is JSON-serialized and escapes HTML delimiters.
        dangerouslySetInnerHTML={{ __html: JSON.stringify(breadcrumb).replace(/</g, "\\u003c") }}
      />
      <header className="max-w-4xl">
        <Badge variant="outline" className="font-mono text-xs uppercase tracking-[0.18em]">
          Query Service read models
        </Badge>
        <h1 className="mt-6 text-balance text-4xl font-semibold tracking-tight text-foreground sm:text-6xl">
          Rebuild current state from recorded facts.
        </h1>
        <p className="mt-6 text-xl leading-9 text-foreground">
          An AllSource projection folds one tenant&apos;s immutable event history into a queryable
          read model. Query Service builds a new generation away from active reads, switches it in
          after completion, then keeps it current from live events.
        </p>
      </header>

      <section aria-labelledby="lifecycle-heading" className="py-14">
        <h2 id="lifecycle-heading" className="text-3xl font-semibold text-foreground">
          Projection lifecycle
        </h2>
        <ol className="mt-8 grid gap-px overflow-hidden border border-border bg-border lg:grid-cols-5">
          {stages.map(([title, detail], index) => (
            <li key={title} className="bg-card p-6">
              <span className="font-mono text-xs text-primary">
                {String(index + 1).padStart(2, "0")}
              </span>
              <h3 className="mt-4 text-lg font-semibold text-foreground">{title}</h3>
              <p className="mt-3 text-sm leading-6 text-muted-foreground">{detail}</p>
            </li>
          ))}
        </ol>
      </section>

      <section className="grid gap-8 border-y border-border py-14 lg:grid-cols-3">
        <div>
          <CircleDot className="size-5 text-primary" />
          <h2 className="mt-4 text-xl font-semibold">Building</h2>
          <p className="mt-3 leading-7 text-muted-foreground">
            Backfill runs against a fixed cutoff while active readers continue using previous
            generation.
          </p>
        </div>
        <div>
          <CheckCircle2 className="size-5 text-primary" />
          <h2 className="mt-4 text-xl font-semibold">Ready</h2>
          <p className="mt-3 leading-7 text-muted-foreground">
            Completed state becomes active as one generation, avoiding half-built read results.
          </p>
        </div>
        <div>
          <Radio className="size-5 text-primary" />
          <h2 className="mt-4 text-xl font-semibold">Live</h2>
          <p className="mt-3 leading-7 text-muted-foreground">
            New events fold into enabled projections and publish entity-state updates over channels.
          </p>
        </div>
      </section>

      <section className="py-14">
        <div className="max-w-3xl">
          <RefreshCcw className="size-6 text-primary" />
          <h2 className="mt-4 text-2xl font-semibold text-foreground">
            Why rebuild instead of migrate rows?
          </h2>
          <p className="mt-4 leading-7 text-muted-foreground">
            Source events keep original history. When read logic changes, a new projection can fold
            that history again and switch generations after verification. Projection state remains
            disposable; Core remains source of truth.
          </p>
        </div>
      </section>

      <div className="flex flex-col gap-4 border-t border-border pt-10 sm:flex-row">
        <Link
          href="/event-sourcing/patterns/projections-read-models"
          className={cn(buttonVariants(), "min-h-12")}
        >
          Read projection pattern <ArrowRight className="ml-2 size-4" />
        </Link>
        <Link href="/examples" className={cn(buttonVariants({ variant: "outline" }), "min-h-12")}>
          Open interactive demo
        </Link>
      </div>
    </div>
  );
}
