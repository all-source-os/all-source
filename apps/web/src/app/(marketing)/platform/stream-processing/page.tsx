import { Badge, buttonVariants, cn } from "@allsource/ui";
import {
  ArrowRight,
  Braces,
  Check,
  CircleDot,
  ExternalLink,
  Filter,
  GitBranch,
  Layers,
  Radio,
  RotateCcw,
  Sigma,
  Split,
  TriangleAlert,
  Workflow,
} from "lucide-react";
import Link from "next/link";
import { breadcrumbSchema, faqPageSchema } from "@/lib/structured-data";

const operators = [
  {
    name: "Filter",
    status: "Available",
    icon: Filter,
    description: "Keep or drop an event by field value using eq, ne, gt, lt, or contains.",
    contract: 'field: "status" · op: "eq" · value: "paid"',
  },
  {
    name: "Map",
    status: "Available",
    icon: Braces,
    description: "Update one field with uppercase, lowercase, trim, multiply, or add transforms.",
    contract: 'field: "currency" · transform: "uppercase"',
  },
  {
    name: "Reduce",
    status: "Available",
    icon: Sigma,
    description: "Maintain count, sum, average, minimum, or maximum state with optional grouping.",
    contract: 'function: "sum" · group_by: "currency"',
  },
  {
    name: "Window",
    status: "Available",
    icon: Workflow,
    description: "Run a nested aggregation over tumbling, sliding, or session windows.",
    contract: "tumbling · sliding · session",
  },
  {
    name: "Branch",
    status: "Available",
    icon: Split,
    description: "Map a string field value to a route name stored on the computed result.",
    contract: 'field: "region" · branches: { "uk": "gbp" }',
  },
  {
    name: "Enrich",
    status: "Scaffold only",
    icon: Layers,
    description:
      "Operator shape exists, but external lookups are not wired. Current Core code emits placeholder values.",
    contract: "Do not use for production joins yet",
  },
] as const;

const pipelineExample = `{
  "id": "7af7b1c8-6165-4c42-9081-e9d20d811780",
  "name": "daily-revenue",
  "description": "Paid revenue grouped by currency",
  "source_event_types": ["order.placed"],
  "operators": [
    {
      "type": "filter",
      "field": "status",
      "value": "paid",
      "op": "eq"
    },
    {
      "type": "window",
      "config": {
        "window_type": "tumbling",
        "size_seconds": 86400,
        "slide_seconds": null,
        "session_timeout_seconds": null
      },
      "aggregation": {
        "type": "reduce",
        "field": "total",
        "function": "sum",
        "group_by": "currency"
      }
    }
  ],
  "enabled": true,
  "output": "daily-revenue-by-currency"
}`;

const lifecycleEndpoints = [
  ["POST", "/api/v1/pipelines", "Register an in-memory pipeline"],
  ["GET", "/api/v1/pipelines", "List registered definitions"],
  ["GET", "/api/v1/pipelines/stats", "Read processing statistics"],
  ["GET", "/api/v1/pipelines/{id}", "Read one definition"],
  ["GET", "/api/v1/pipelines/{id}/stats", "Read one pipeline's statistics"],
  ["PUT", "/api/v1/pipelines/{id}/reset", "Clear operator state and counters"],
  ["DELETE", "/api/v1/pipelines/{id}", "Remove a definition"],
] as const;

const streamProcessingFaqs = [
  {
    question: "What is AllSource stream processing?",
    answer:
      "AllSource Core can match accepted event types and run an ordered, in-process pipeline of Filter, Map, Reduce, Window, Branch, and Enrich operators. Source events remain unchanged. Enrich external lookups and automatic output persistence are not complete today.",
  },
  {
    question: "Are AllSource pipeline results persisted automatically?",
    answer:
      "No. Core computes matching pipeline results in process and tracks processing statistics, but the configured output name does not currently persist a projection or publish a topic. Applications that need a durable output must add that integration.",
  },
  {
    question: "Do AllSource pipelines survive a Core restart?",
    answer:
      "Not yet. Pipeline definitions, window buffers, reduction state, and counters live in memory. Register definitions again after restart and rebuild any required state from source events.",
  },
  {
    question: "Can hosted AllSource customers configure pipelines through the public gateway?",
    answer:
      "Not currently. Pipeline lifecycle endpoints exist on the self-hosted Core server. The hosted public gateway does not expose those routes, so this page treats stream processing as a self-hosted Core capability.",
  },
  {
    question: "Does AllSource stream processing replace Kafka or Flink?",
    answer:
      "No. Core pipelines suit transformations that should execute beside the event store. Use Kafka, Redpanda, Flink, or another stream platform when you need cross-service transport, independent scaling, connector ecosystems, durable output topics, or separate consumer offsets.",
  },
] as const;

function JsonLd({ value }: { value: object }) {
  return (
    <script
      type="application/ld+json"
      // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD requires a script tag; '<' is escaped before insertion
      dangerouslySetInnerHTML={{ __html: JSON.stringify(value).replace(/</g, "\\u003c") }}
    />
  );
}

function PipelineRail() {
  const stages = [
    ["match", "order.placed"],
    ["filter", "status = paid"],
    ["window", "tumbling · 86400s"],
    ["reduce", "sum total · currency"],
  ] as const;

  return (
    <figure className="border border-border bg-card">
      <div className="flex items-center justify-between border-b border-border px-5 py-3">
        <span className="font-mono text-xs uppercase tracking-[0.18em] text-muted-foreground">
          Core ingestion path
        </span>
        <CircleDot className="h-4 w-4 text-primary" aria-hidden="true" />
      </div>
      <div className="p-5 sm:p-6">
        <div className="border border-border bg-background px-4 py-3">
          <p className="font-mono text-[0.68rem] uppercase tracking-[0.16em] text-muted-foreground">
            Accepted source event
          </p>
          <p className="mt-2 font-mono text-sm text-foreground">order.placed</p>
        </div>
        <ol className="ml-5 border-l border-primary/50 py-3">
          {stages.map(([label, value], index) => (
            <li key={label} className="relative py-2 pl-7">
              <span
                className="absolute -left-1 top-1/2 h-2 w-2 -translate-y-1/2 rounded-full bg-primary ring-4 ring-background"
                aria-hidden="true"
              />
              <div className="grid grid-cols-[4rem_1fr] gap-3 border border-border bg-background px-3 py-2.5">
                <span className="font-mono text-[0.68rem] uppercase tracking-[0.14em] text-primary">
                  {String(index + 1).padStart(2, "0")} {label}
                </span>
                <span className="font-mono text-xs text-foreground">{value}</span>
              </div>
            </li>
          ))}
        </ol>
        <div className="border border-primary/40 bg-primary/5 px-4 py-3">
          <div className="flex items-center justify-between gap-4">
            <p className="font-mono text-[0.68rem] uppercase tracking-[0.16em] text-primary">
              Computed result
            </p>
            <span className="font-mono text-[0.68rem] text-muted-foreground">in process</span>
          </div>
          <p className="mt-2 font-mono text-sm text-foreground">
            {`{ "group": "GBP", "value": 4827 }`}
          </p>
        </div>
      </div>
      <figcaption className="grid gap-px border-t border-border bg-border sm:grid-cols-3">
        {[
          ["Source event", "Stored"],
          ["Counters", "Tracked"],
          ["Result sink", "Not automatic"],
        ].map(([label, value]) => (
          <div key={label} className="bg-card px-4 py-3">
            <p className="font-mono text-[0.65rem] uppercase tracking-[0.14em] text-muted-foreground">
              {label}
            </p>
            <p className="mt-1 text-xs font-medium text-foreground">{value}</p>
          </div>
        ))}
      </figcaption>
    </figure>
  );
}

export default function StreamProcessingPage() {
  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "Event store", path: "/platform/event-sourcing" },
    { name: "Stream processing", path: "/platform/stream-processing" },
  ]);

  return (
    <div className="overflow-hidden">
      <JsonLd value={breadcrumb} />
      <JsonLd value={faqPageSchema(streamProcessingFaqs)} />

      <section className="mx-auto grid w-full max-w-6xl gap-12 px-4 pb-16 pt-20 sm:px-6 lg:grid-cols-[1.04fr_0.96fr] lg:items-center lg:px-8 lg:pb-24 lg:pt-28">
        <div>
          <div className="flex flex-wrap items-center gap-3">
            <Badge variant="outline" className="font-mono text-xs uppercase tracking-[0.18em]">
              Self-hosted Core capability
            </Badge>
            <span className="font-mono text-xs text-muted-foreground">v0.5+</span>
          </div>
          <h1 className="mt-6 max-w-3xl text-balance text-4xl font-semibold leading-[1.05] tracking-tight text-foreground sm:text-6xl">
            Transform event streams beside the source history.
          </h1>
          <p className="mt-6 max-w-2xl text-pretty text-lg leading-8 text-muted-foreground">
            AllSource Core matches accepted event types and runs ordered Filter, Map, Reduce,
            Window, and Branch operators in process. Source events stay unchanged; pipeline counters
            remain inspectable.
          </p>
          <div className="mt-6 flex items-start gap-3 border-l-2 border-amber-500/70 pl-4 text-sm leading-6 text-muted-foreground">
            <TriangleAlert className="mt-1 h-4 w-4 shrink-0 text-amber-500" aria-hidden="true" />
            <p>
              Current boundary: results are not persisted or published automatically, definitions
              and state are in memory, and hosted gateway routes are not exposed.
            </p>
          </div>
          <div className="mt-8 flex flex-col gap-3 sm:flex-row">
            <Link
              href="https://github.com/all-source-os/all-source/blob/main/apps/core/src/application/services/pipeline.rs"
              className={cn(buttonVariants({ variant: "default" }))}
            >
              Inspect Core implementation
              <ExternalLink className="ml-2 h-4 w-4" aria-hidden="true" />
            </Link>
            <Link href="/docs/api" className={cn(buttonVariants({ variant: "outline" }))}>
              Core API reference
            </Link>
          </div>
        </div>
        <PipelineRail />
      </section>

      <section aria-labelledby="contract-heading" className="border-y border-border bg-muted/15">
        <div className="mx-auto w-full max-w-6xl px-4 py-10 sm:px-6 lg:px-8">
          <div className="grid gap-8 lg:grid-cols-[0.6fr_1.4fr] lg:items-start">
            <div>
              <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
                Execution contract
              </p>
              <h2 id="contract-heading" className="mt-3 text-2xl font-semibold text-foreground">
                Know where state lives.
              </h2>
            </div>
            <dl className="grid gap-px bg-border sm:grid-cols-3">
              {[
                [
                  "Where",
                  "Inside AllSource Core, on each accepted event that matches `source_event_types`.",
                ],
                [
                  "State",
                  "Reduce values, window buffers, definitions, and counters remain in memory.",
                ],
                [
                  "Restart",
                  "Register pipelines again; replay source events when derived state must be rebuilt.",
                ],
              ].map(([term, detail]) => (
                <div key={term} className="bg-background p-5">
                  <dt className="font-mono text-xs uppercase tracking-[0.16em] text-primary">
                    {term}
                  </dt>
                  <dd className="mt-3 text-sm leading-6 text-muted-foreground">{detail}</dd>
                </div>
              ))}
            </dl>
          </div>
        </div>
      </section>

      <section
        aria-labelledby="operators-heading"
        className="mx-auto w-full max-w-6xl px-4 py-16 sm:px-6 sm:py-24 lg:px-8"
      >
        <div className="grid gap-8 lg:grid-cols-[0.62fr_1.38fr] lg:gap-16">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Ordered operators
            </p>
            <h2 id="operators-heading" className="mt-3 text-3xl font-semibold text-foreground">
              Each result feeds the next stage.
            </h2>
            <p className="mt-4 text-base leading-7 text-muted-foreground">
              Operators execute in array order. Filter can halt processing; stateful operators keep
              values inside that pipeline instance.
            </p>
          </div>
          <ol className="border-y border-border">
            {operators.map((operator, index) => {
              const Icon = operator.icon;
              const limited = operator.status !== "Available";
              return (
                <li
                  key={operator.name}
                  className="grid gap-4 border-b border-border py-5 last:border-0 sm:grid-cols-[2.5rem_8rem_1fr] sm:items-start sm:gap-5"
                >
                  <div className="flex items-center justify-between sm:block">
                    <span className="font-mono text-xs text-muted-foreground">
                      {String(index + 1).padStart(2, "0")}
                    </span>
                    <Icon
                      className={cn("h-4 w-4 sm:mt-3", limited ? "text-amber-500" : "text-primary")}
                      aria-hidden="true"
                    />
                  </div>
                  <div>
                    <h3 className="font-semibold text-foreground">{operator.name}</h3>
                    <span
                      className={cn(
                        "mt-2 inline-flex border px-2 py-1 font-mono text-[0.65rem] uppercase tracking-[0.14em]",
                        limited
                          ? "border-amber-500/40 text-amber-500"
                          : "border-primary/30 text-primary"
                      )}
                    >
                      {operator.status}
                    </span>
                  </div>
                  <div>
                    <p className="text-sm leading-6 text-foreground">{operator.description}</p>
                    <p className="mt-2 font-mono text-xs leading-5 text-muted-foreground">
                      {operator.contract}
                    </p>
                  </div>
                </li>
              );
            })}
          </ol>
        </div>
      </section>

      <section aria-labelledby="definition-heading" className="border-y border-border bg-muted/15">
        <div className="mx-auto grid w-full max-w-6xl gap-10 px-4 py-16 sm:px-6 sm:py-24 lg:grid-cols-[1.05fr_0.95fr] lg:gap-16 lg:px-8">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Valid Core payload
            </p>
            <h2 id="definition-heading" className="mt-3 text-3xl font-semibold text-foreground">
              Register a typed pipeline definition.
            </h2>
            <p className="mt-4 max-w-xl text-base leading-7 text-muted-foreground">
              This example matches the current Rust `PipelineConfig` serde shape, including required
              `id` and `output` fields and nested window aggregation.
            </p>
            <div className="mt-8 overflow-hidden border border-border bg-[#0b1522]">
              <div className="flex items-center justify-between border-b border-white/10 px-4 py-3">
                <span className="font-mono text-xs text-white/60">daily-revenue.json</span>
                <span className="font-mono text-[0.65rem] uppercase tracking-[0.16em] text-cyan-400">
                  POST /api/v1/pipelines
                </span>
              </div>
              <pre className="max-h-[34rem] overflow-auto p-5 text-xs leading-6 text-slate-300 sm:text-sm">
                <code>{pipelineExample}</code>
              </pre>
            </div>
          </div>

          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Lifecycle endpoints
            </p>
            <h2 className="mt-3 text-3xl font-semibold text-foreground">Operate through Core.</h2>
            <p className="mt-4 text-base leading-7 text-muted-foreground">
              These routes live on Core's internal server, normally port 3900. Keep Core behind your
              network boundary.
            </p>
            <div className="mt-8 border-y border-border">
              {lifecycleEndpoints.map(([method, path, description]) => (
                <div
                  key={`${method}-${path}`}
                  className="grid gap-2 border-b border-border py-4 last:border-0"
                >
                  <div className="flex flex-wrap items-center gap-3">
                    <span
                      className={cn(
                        "min-w-14 border px-2 py-1 text-center font-mono text-[0.65rem] font-semibold",
                        method === "GET"
                          ? "border-primary/30 text-primary"
                          : "border-emerald-500/30 text-emerald-500"
                      )}
                    >
                      {method}
                    </span>
                    <code className="break-all text-xs text-foreground sm:text-sm">{path}</code>
                  </div>
                  <p className="pl-0 text-sm text-muted-foreground sm:pl-[4.25rem]">
                    {description}
                  </p>
                </div>
              ))}
            </div>
            <div className="mt-8 flex items-start gap-3 border border-amber-500/30 bg-amber-500/5 p-4">
              <TriangleAlert
                className="mt-0.5 h-4 w-4 shrink-0 text-amber-500"
                aria-hidden="true"
              />
              <p className="text-sm leading-6 text-muted-foreground">
                `output` is currently descriptive configuration. It does not create a projection,
                write another event, or publish a topic.
              </p>
            </div>
          </div>
        </div>
      </section>

      <section
        aria-labelledby="fit-heading"
        className="mx-auto w-full max-w-6xl px-4 py-16 sm:px-6 sm:py-24 lg:px-8"
      >
        <div className="grid gap-8 lg:grid-cols-[0.62fr_1.38fr] lg:gap-16">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              System boundary
            </p>
            <h2 id="fit-heading" className="mt-3 text-3xl font-semibold text-foreground">
              Pick by delivery requirement.
            </h2>
            <p className="mt-4 text-base leading-7 text-muted-foreground">
              “Stream processing” covers several jobs. AllSource Core owns one of them: ordered
              computation beside accepted history.
            </p>
          </div>
          <div className="grid gap-px border border-border bg-border sm:grid-cols-2">
            {[
              {
                icon: Check,
                title: "Core pipelines",
                copy: "Use for inline filtering, field transforms, aggregation, and routing calculations beside source events.",
                link: "/platform/event-sourcing",
                label: "Understand Core",
              },
              {
                icon: Radio,
                title: "Query Service WebSockets",
                copy: "Use for live accepted-event feeds to clients. These feeds do not publish Core pipeline results.",
                link: "/solutions/real-time-analytics",
                label: "See real-time analytics",
              },
              {
                icon: RotateCcw,
                title: "Replay and projections",
                copy: "Use source events to rebuild queryable state. This remains separate from a pipeline's configured output name.",
                link: "/use-cases#event-replay",
                label: "See replay use case",
              },
              {
                icon: GitBranch,
                title: "Kafka, Redpanda, or Flink",
                copy: "Use when transport, durable output topics, connectors, consumer offsets, or independent scaling are primary.",
              },
            ].map((item) => {
              const Icon = item.icon;
              return (
                <article key={item.title} className="bg-background p-6">
                  <Icon className="h-5 w-5 text-primary" aria-hidden="true" />
                  <h3 className="mt-4 font-semibold text-foreground">{item.title}</h3>
                  <p className="mt-3 text-sm leading-6 text-muted-foreground">{item.copy}</p>
                  {item.link && item.label ? (
                    <Link
                      href={item.link}
                      className="mt-5 inline-flex items-center gap-2 text-sm font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    >
                      {item.label}
                      <ArrowRight className="h-4 w-4" aria-hidden="true" />
                    </Link>
                  ) : null}
                </article>
              );
            })}
          </div>
        </div>
      </section>

      <section aria-labelledby="faq-heading" className="border-y border-border bg-muted/15">
        <div className="mx-auto w-full max-w-6xl px-4 py-16 sm:px-6 sm:py-24 lg:px-8">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Direct answers
          </p>
          <h2 id="faq-heading" className="mt-3 text-3xl font-semibold text-foreground">
            Stream-processing questions
          </h2>
          <div className="mt-8 grid gap-px overflow-hidden border border-border bg-border lg:grid-cols-2">
            {streamProcessingFaqs.map((faq) => (
              <article key={faq.question} className="bg-background p-6 sm:p-8">
                <h3 className="font-semibold text-foreground">{faq.question}</h3>
                <p className="mt-3 text-sm leading-6 text-muted-foreground">{faq.answer}</p>
              </article>
            ))}
          </div>
        </div>
      </section>

      <section className="mx-auto w-full max-w-6xl px-4 py-16 sm:px-6 sm:py-24 lg:px-8">
        <div className="flex flex-col justify-between gap-8 border-l-2 border-primary pl-6 sm:flex-row sm:items-end sm:pl-8">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Build from evidence
            </p>
            <h2 className="mt-3 max-w-2xl text-3xl font-semibold text-foreground">
              Test your own pipeline against retained events.
            </h2>
            <p className="mt-3 max-w-2xl text-sm leading-6 text-muted-foreground">
              Published 469K events/sec result measures Core batch ingestion, not arbitrary operator
              chains. Benchmark your definition, durability mode, and hardware.
            </p>
          </div>
          <div className="flex flex-col gap-3 sm:flex-row">
            <Link
              href="https://github.com/all-source-os/all-source"
              className={cn(buttonVariants({ variant: "default" }))}
            >
              Self-host Core
              <ExternalLink className="ml-2 h-4 w-4" aria-hidden="true" />
            </Link>
            <Link
              href="/blog/reproduce-the-469k-events-benchmark"
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              Read benchmark method
            </Link>
          </div>
        </div>
      </section>
    </div>
  );
}
