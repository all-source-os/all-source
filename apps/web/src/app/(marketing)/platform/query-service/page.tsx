import { Badge, buttonVariants, cn } from "@allsource/ui";
import { Activity, ArrowRight, BarChart3, Braces, Radio, RefreshCcw } from "lucide-react";
import Link from "next/link";
import { breadcrumbSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "AllSource Query Service: Realtime Reads and Analytics",
  description:
    "Query Service serves tenant-scoped HTTP queries, realtime channels, analytics, and rebuildable projections over durable AllSource Core events.",
  canonical: "/platform/query-service",
});

const lanes = [
  {
    icon: Braces,
    label: "HTTP",
    job: "Request-response reads",
    detail:
      "Tenant-scoped event queries, entity state, projected reads, health, and OpenAPI-backed endpoints.",
  },
  {
    icon: Radio,
    label: "Realtime",
    job: "Live interface updates",
    detail:
      "Phoenix Channels broadcast accepted events and projection-state updates to subscribed clients.",
  },
  {
    icon: BarChart3,
    label: "Analytics",
    job: "Aggregate questions",
    detail:
      "Dedicated analytics routes answer frequency, summary, correlation, usage, and operational queries.",
  },
  {
    icon: RefreshCcw,
    label: "Projections",
    job: "Current-state read models",
    detail:
      "Tenant-scoped folds build from Core history, update from live events, and can be rebuilt without replacing source truth.",
  },
] as const;

export default function QueryServicePage() {
  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "Platform", path: "/platform/event-sourcing" },
    { name: "Query Service", path: "/platform/query-service" },
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
          Read plane · no second database
        </Badge>
        <h1 className="mt-6 text-balance text-4xl font-semibold tracking-tight text-foreground sm:text-6xl">
          One event history. Four read paths.
        </h1>
        <p className="mt-6 text-xl leading-9 text-foreground">
          AllSource Query Service is the stateless Elixir/Phoenix read plane over Core. It separates
          HTTP request-response queries, realtime channels, analytics endpoints, and rebuildable
          projections so each consumer gets the read shape it needs without creating another source
          of truth.
        </p>
      </header>

      <figure className="mt-12 overflow-hidden border border-border bg-card">
        <div className="grid gap-px bg-border lg:grid-cols-[0.8fr_repeat(4,1fr)_0.8fr]">
          <div className="grid min-h-28 place-items-center bg-primary/10 p-4 text-center">
            <div>
              <Activity className="mx-auto size-5 text-primary" />
              <p className="mt-2 font-mono text-xs">Core events</p>
            </div>
          </div>
          {lanes.map(({ icon: Icon, label }) => (
            <div key={label} className="grid min-h-28 place-items-center bg-card p-4 text-center">
              <div>
                <Icon className="mx-auto size-5 text-primary" />
                <p className="mt-2 font-mono text-xs">{label}</p>
              </div>
            </div>
          ))}
          <div className="grid min-h-28 place-items-center bg-primary/10 p-4 text-center">
            <p className="font-mono text-xs">Apps + agents</p>
          </div>
        </div>
        <figcaption className="border-t border-border px-5 py-3 text-sm text-muted-foreground">
          Query Service caches and read models are disposable. Accepted events remain durable in
          Core.
        </figcaption>
      </figure>

      <section aria-labelledby="lanes-heading" className="py-14">
        <h2 id="lanes-heading" className="text-3xl font-semibold text-foreground">
          Choose by read job
        </h2>
        <div className="mt-8 grid gap-px overflow-hidden border border-border bg-border md:grid-cols-2">
          {lanes.map(({ icon: Icon, label, job, detail }) => (
            <section key={label} className="bg-card p-6 sm:p-8">
              <div className="flex items-center gap-3">
                <Icon className="size-5 text-primary" />
                <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
                  {label}
                </p>
              </div>
              <h3 className="mt-4 text-xl font-semibold text-foreground">{job}</h3>
              <p className="mt-3 leading-7 text-muted-foreground">{detail}</p>
            </section>
          ))}
        </div>
      </section>

      <section className="border-y border-border py-14">
        <h2 className="text-2xl font-semibold text-foreground">
          Boundary that prevents data drift
        </h2>
        <p className="mt-5 max-w-3xl leading-7 text-muted-foreground">
          Query Service owns delivery and read shape, not durable source data. ETS caches and tenant
          projection state rebuild from Core history. This keeps realtime, analytics, and HTTP
          concerns independently operable without synchronising PostgreSQL or another primary
          database.
        </p>
      </section>

      <div className="flex flex-col gap-4 py-14 sm:flex-row">
        <Link href="/platform/projections" className={cn(buttonVariants(), "min-h-12")}>
          See projection lifecycle <ArrowRight className="ml-2 size-4" />
        </Link>
        <Link href="/docs/api" className={cn(buttonVariants({ variant: "outline" }), "min-h-12")}>
          Open API reference
        </Link>
      </div>
    </div>
  );
}
