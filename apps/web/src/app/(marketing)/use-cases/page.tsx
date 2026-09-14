import { Badge, buttonVariants, cn } from "@allsource/ui";
import {
  ArrowRight,
  Brain,
  Check,
  CircleDollarSign,
  Database,
  GitBranch,
  History,
  Play,
  RotateCcw,
  ShieldCheck,
} from "lucide-react";
import Link from "next/link";
import { type ProductVerticalId, productVerticals } from "@/lib/product-verticals";
import { breadcrumbSchema, faqPageSchema } from "@/lib/structured-data";

type UseCase = {
  id: string;
  number: string;
  eyebrow: string;
  title: string;
  answer: string;
  trigger: string;
  events: readonly string[];
  outcome: string;
  capabilities: readonly string[];
  products: readonly ProductVerticalId[];
  href: string;
  linkLabel: string;
  icon: typeof ShieldCheck;
};

const useCases: readonly UseCase[] = [
  {
    id: "audit-trails",
    number: "01",
    eyebrow: "Prove",
    title: "Audit trails and compliance",
    answer:
      "Record each accepted change with its actor, timestamp, entity, and metadata. Query the ordered history instead of reconstructing it from overwritten rows and scattered logs.",
    trigger: "When someone asks what changed, who changed it, and what the system knew then.",
    events: ["customer.created", "risk.reviewed", "policy.approved"],
    outcome: "Export the source events and reconstruct state at the review timestamp.",
    capabilities: [
      "Append-only events with integrity checks",
      "Point-in-time state through `as_of` queries",
      "Tenant-scoped HTTP history through Query Service",
    ],
    products: ["core", "query", "hosted"],
    href: "/solutions/audit-compliance",
    linkLabel: "Explore audit and compliance",
    icon: ShieldCheck,
  },
  {
    id: "event-replay",
    number: "02",
    eyebrow: "Reproduce",
    title: "Event replay and debugging",
    answer:
      "Replay the exact production sequence against a fresh projection. The first divergent state gives engineers a smaller, testable explanation for a failure.",
    trigger: "When current state shows the failure, but not the sequence that caused it.",
    events: ["checkout.started", "payment.authorized", "inventory.rejected"],
    outcome: "Rebuild the checkout projection and compare state before and after rejection.",
    capabilities: [
      "Deterministic replay from ordered events",
      "Query Service rebuilds read models without rewriting source history",
      "Published Core reference: 11.9µs p99 indexed reads",
    ],
    products: ["core", "query", "hosted"],
    href: "/event-replay-debugging",
    linkLabel: "See event replay debugging",
    icon: RotateCcw,
  },
  {
    id: "agent-memory",
    number: "03",
    eyebrow: "Remember",
    title: "AI-agent memory with provenance",
    answer:
      "Let an agent recall decisions across sessions while retaining the source events behind each memory. Prime derives graph, vector, and temporal context; Core can persist the record.",
    trigger:
      "When an agent must remember more than the current prompt and explain where recall came from.",
    events: ["decision.recorded", "constraint.changed", "memory.recalled"],
    outcome:
      "Return the relevant decision with its relationships, timestamp, and source-event trail.",
    capabilities: [
      "Graph, vector, and temporal retrieval in Prime",
      "Durable source-event provenance through Core",
      "Agent access through separate MCP connectors",
    ],
    products: ["prime", "core", "mcp"],
    href: "/solutions/agent-memory",
    linkLabel: "Explore agent memory",
    icon: Brain,
  },
  {
    id: "financial-history",
    number: "04",
    eyebrow: "Reconcile",
    title: "Financial transaction history",
    answer:
      "Store debits, credits, reversals, and corrections as ordered events, then derive balances in a projection. Corrections remain visible instead of replacing prior facts.",
    trigger: "When a balance or transaction must be explainable at any point in its history.",
    events: ["debit.posted", "fee.assessed", "credit.reversed"],
    outcome: "Replay the ledger to show when the balance moved and which event caused it.",
    capabilities: [
      "Append-only ledger history",
      "HTTP, realtime, and analytics reads over balance projections",
      "Published Core reference: 469K events/sec batch ingest",
    ],
    products: ["core", "query", "hosted"],
    href: "/solutions/financial-services",
    linkLabel: "Explore financial services",
    icon: CircleDollarSign,
  },
] as const;

const useCaseFaqs = [
  {
    question: "What role does Query Service play in these use cases?",
    answer:
      "Core remains the durable source of truth. Query Service provides tenant-scoped HTTP queries, realtime event and projection channels, analytics endpoints, and rebuildable read models tailored to each consuming application.",
  },
  {
    question: "What are the main AllSource Event Store use cases?",
    answer:
      "AllSource is designed for audit trails, event replay and debugging, AI-agent memory with provenance, and financial transaction history. These workloads benefit from ordered, immutable events and point-in-time reconstruction.",
  },
  {
    question: "Does AllSource require PostgreSQL?",
    answer:
      "No. AllSource Core is the database for event history and event-sourced operational metadata, including tenants, users, API keys, configuration, subscriptions, quotas, and billing state. Current AllSource services run without PostgreSQL.",
  },
  {
    question: "Which AllSource product handles AI-agent memory?",
    answer:
      "AllSource Prime is the agent-memory engine. It combines graph relationships, vector retrieval, temporal context, and source-event provenance. Core is the event-store database; MCP connectors expose explicit tools to compatible agents.",
  },
  {
    question: "Can one AllSource deployment serve multiple projects?",
    answer:
      "Yes. Tenant-scoped streams, API keys, system metadata, and projections let one AllSource deployment isolate and serve multiple products. External databases and streaming systems are optional integrations, not prerequisites.",
  },
] as const;

const productById = new Map(productVerticals.map((product) => [product.id, product]));

function JsonLd({ value }: { value: object }) {
  return (
    <script
      type="application/ld+json"
      // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD requires a script tag; '<' is escaped before insertion
      dangerouslySetInnerHTML={{ __html: JSON.stringify(value).replace(/</g, "\\u003c") }}
    />
  );
}

function EventTrace({ useCase }: { useCase: UseCase }) {
  return (
    <figure className="border border-border bg-card">
      <div className="flex items-center justify-between border-b border-border px-5 py-3">
        <span className="font-mono text-xs uppercase tracking-[0.18em] text-muted-foreground">
          Ordered event stream
        </span>
        <History className="h-4 w-4 text-primary" aria-hidden="true" />
      </div>
      <ol className="px-5 py-2">
        {useCase.events.map((event, index) => (
          <li key={event} className="grid grid-cols-[2rem_1fr] gap-3">
            <div className="flex flex-col items-center" aria-hidden="true">
              <span className="mt-5 h-2 w-2 rounded-full border border-primary bg-background" />
              {index < useCase.events.length - 1 ? (
                <span className="min-h-8 w-px flex-1 bg-border" />
              ) : null}
            </div>
            <div className="border-b border-border py-4 last:border-0">
              <span className="font-mono text-xs text-muted-foreground">
                +{String(index).padStart(2, "0")}:0{index + 1}
              </span>
              <p className="mt-1 font-mono text-sm text-foreground">{event}</p>
            </div>
          </li>
        ))}
      </ol>
      <figcaption className="border-t border-primary/30 bg-primary/5 p-5">
        <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">Derived result</p>
        <p className="mt-2 text-sm leading-6 text-foreground">{useCase.outcome}</p>
      </figcaption>
    </figure>
  );
}

export default function UseCasesPage() {
  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "Use cases", path: "/use-cases" },
  ]);

  return (
    <main className="overflow-hidden">
      <JsonLd value={breadcrumb} />
      <JsonLd value={faqPageSchema(useCaseFaqs)} />

      <section className="mx-auto grid w-full max-w-6xl gap-12 px-4 pb-14 pt-20 sm:px-6 lg:grid-cols-[1.08fr_0.92fr] lg:items-end lg:px-8 lg:pb-20 lg:pt-28">
        <div>
          <Badge variant="outline" className="font-mono text-xs uppercase tracking-[0.18em]">
            AllSource use cases
          </Badge>
          <h1 className="mt-6 max-w-3xl text-balance text-4xl font-semibold leading-[1.05] tracking-tight text-foreground sm:text-6xl">
            Keep the history your system will need later.
          </h1>
          <p className="mt-6 max-w-2xl text-pretty text-lg leading-8 text-muted-foreground">
            AllSource records ordered events so teams can prove what happened, replay how state
            changed, give agents source-backed memory, and reconstruct financial history.
          </p>
          <div className="mt-8 flex flex-col gap-3 sm:flex-row">
            <Link href="/signup" className={cn(buttonVariants({ variant: "default" }))}>
              Start hosted trial
              <ArrowRight className="ml-2 h-4 w-4" aria-hidden="true" />
            </Link>
            <Link
              href="https://github.com/all-source-os/all-source"
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              Self-host Core
            </Link>
          </div>
        </div>

        <figure className="border border-border bg-card">
          <div className="flex items-center justify-between border-b border-border px-5 py-3">
            <span className="font-mono text-xs uppercase tracking-[0.18em] text-muted-foreground">
              One history · four outcomes
            </span>
            <GitBranch className="h-4 w-4 text-primary" aria-hidden="true" />
          </div>
          <ol className="divide-y divide-border">
            {useCases.map((useCase) => {
              const Icon = useCase.icon;
              return (
                <li key={useCase.id}>
                  <Link
                    href={`#${useCase.id}`}
                    className="group grid grid-cols-[2.25rem_1fr_auto] items-center gap-3 px-5 py-4 transition-colors hover:bg-primary/5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
                  >
                    <Icon className="h-4 w-4 text-primary" aria-hidden="true" />
                    <span>
                      <span className="block font-mono text-[0.68rem] uppercase tracking-[0.16em] text-muted-foreground">
                        {useCase.number} · {useCase.eyebrow}
                      </span>
                      <span className="mt-1 block text-sm font-medium text-foreground">
                        {useCase.title}
                      </span>
                    </span>
                    <ArrowRight
                      className="h-4 w-4 text-muted-foreground transition-transform group-hover:translate-x-1"
                      aria-hidden="true"
                    />
                  </Link>
                </li>
              );
            })}
          </ol>
        </figure>
      </section>

      <section aria-labelledby="workloads-heading" className="border-y border-border bg-muted/15">
        <div className="mx-auto w-full max-w-6xl px-4 py-8 sm:px-6 lg:px-8">
          <div className="grid gap-4 md:grid-cols-[0.55fr_1.45fr] md:items-center">
            <div>
              <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
                Choose by problem
              </p>
              <h2 id="workloads-heading" className="mt-2 text-xl font-semibold text-foreground">
                What must remain explainable?
              </h2>
            </div>
            <nav aria-label="Use-case sections" className="grid gap-px bg-border sm:grid-cols-2">
              {useCases.map((useCase) => (
                <Link
                  key={useCase.id}
                  href={`#${useCase.id}`}
                  className="flex items-center justify-between gap-4 bg-background px-4 py-3 text-sm text-foreground hover:bg-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
                >
                  <span>{useCase.title}</span>
                  <span className="font-mono text-xs text-primary">{useCase.number}</span>
                </Link>
              ))}
            </nav>
          </div>
        </div>
      </section>

      <div className="mx-auto w-full max-w-6xl px-4 sm:px-6 lg:px-8">
        {useCases.map((useCase, index) => {
          const Icon = useCase.icon;
          return (
            <section
              key={useCase.id}
              id={useCase.id}
              aria-labelledby={`${useCase.id}-heading`}
              className="scroll-mt-24 border-b border-border py-16 sm:py-24"
            >
              <div className="grid gap-10 lg:grid-cols-2 lg:gap-16">
                <div className={cn(index % 2 === 1 && "lg:order-2")}>
                  <div className="flex items-center gap-3">
                    <Icon className="h-5 w-5 text-primary" aria-hidden="true" />
                    <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
                      {useCase.number} · {useCase.eyebrow}
                    </p>
                  </div>
                  <h2
                    id={`${useCase.id}-heading`}
                    className="mt-4 text-balance text-3xl font-semibold tracking-tight text-foreground sm:text-4xl"
                  >
                    {useCase.title}
                  </h2>
                  <p className="mt-5 text-lg leading-8 text-foreground">{useCase.answer}</p>
                  <p className="mt-5 border-l-2 border-primary pl-4 text-sm leading-6 text-muted-foreground">
                    <strong className="font-medium text-foreground">Use when: </strong>
                    {useCase.trigger}
                  </p>

                  <ul className="mt-7 space-y-3">
                    {useCase.capabilities.map((capability) => (
                      <li key={capability} className="flex items-start gap-3 text-sm leading-6">
                        <Check className="mt-1 h-4 w-4 shrink-0 text-primary" aria-hidden="true" />
                        <span>{capability}</span>
                      </li>
                    ))}
                  </ul>

                  <ul className="mt-7 flex flex-wrap gap-2" aria-label="Relevant products">
                    {useCase.products.map((productId) => {
                      const product = productById.get(productId);
                      if (!product) return null;
                      return (
                        <li key={product.id}>
                          <Link
                            href={product.path}
                            className="block border border-border px-3 py-1.5 font-mono text-xs text-muted-foreground hover:border-primary hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                            title={product.directAnswer}
                          >
                            {product.role}: {product.name.replace("AllSource ", "")}
                          </Link>
                        </li>
                      );
                    })}
                  </ul>

                  <Link
                    href={useCase.href}
                    className="mt-8 inline-flex items-center gap-2 text-sm font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    {useCase.linkLabel}
                    <ArrowRight className="h-4 w-4" aria-hidden="true" />
                  </Link>
                </div>
                <div className={cn(index % 2 === 1 && "lg:order-1")}>
                  <EventTrace useCase={useCase} />
                </div>
              </div>
            </section>
          );
        })}
      </div>

      <section aria-labelledby="fit-heading" className="border-b border-border bg-muted/15">
        <div className="mx-auto w-full max-w-6xl px-4 py-16 sm:px-6 sm:py-24 lg:px-8">
          <div className="grid gap-10 lg:grid-cols-[0.7fr_1.3fr] lg:gap-16">
            <div>
              <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
                Architecture fit
              </p>
              <h2 id="fit-heading" className="mt-3 text-3xl font-semibold text-foreground">
                One event-native data layer across products.
              </h2>
              <p className="mt-4 text-base leading-7 text-muted-foreground">
                Core owns durable source history and event-sourced operational metadata. Tenant
                boundaries let one deployment serve multiple projects without adding a database per
                application.
              </p>
            </div>
            <div className="border-y border-border">
              {[
                {
                  icon: Check,
                  label: "Core is primary",
                  copy: "Events and system metadata share one WAL-durable source of truth.",
                  positive: true,
                },
                {
                  icon: Database,
                  label: "Serve many projects",
                  copy: "Separate products with tenant-scoped streams, keys, metadata, and projections.",
                  positive: true,
                },
                {
                  icon: RotateCcw,
                  label: "Derive current state",
                  copy: "Query Service builds tenant-scoped read models from retained Core events and rebuilds them through replay.",
                  positive: true,
                },
                {
                  icon: GitBranch,
                  label: "Integrate at edges",
                  copy: "Connect external transport or specialist systems only when a workload explicitly needs them.",
                },
              ].map((item) => {
                const Icon = item.icon;
                return (
                  <div
                    key={item.label}
                    className="grid gap-2 border-b border-border py-5 last:border-0 sm:grid-cols-[12rem_1fr] sm:gap-6"
                  >
                    <div className="flex items-center gap-2 font-medium text-foreground">
                      <Icon
                        className={cn(
                          "h-4 w-4",
                          item.positive ? "text-primary" : "text-muted-foreground"
                        )}
                        aria-hidden="true"
                      />
                      {item.label}
                    </div>
                    <p className="text-sm leading-6 text-muted-foreground">{item.copy}</p>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      </section>

      <section
        aria-labelledby="demo-heading"
        className="mx-auto w-full max-w-6xl px-4 py-16 sm:px-6 sm:py-24 lg:px-8"
      >
        <div className="mb-8 flex flex-col justify-between gap-5 sm:flex-row sm:items-end">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Product walkthrough
            </p>
            <h2 id="demo-heading" className="mt-3 text-3xl font-semibold text-foreground">
              See event history become inspectable state.
            </h2>
          </div>
          <Link
            href="/examples"
            className="inline-flex items-center gap-2 text-sm font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            Open working examples
            <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </Link>
        </div>
        <div className="relative overflow-hidden border border-border bg-card">
          <video
            className="aspect-video w-full bg-background object-cover"
            controls
            muted
            playsInline
            preload="none"
            poster="/assets/hero-screenshot.webp"
            aria-label="AllSource event explorer product walkthrough"
          >
            <source src="/assets/demo-video.mp4" type="video/mp4" />
          </video>
          <div className="pointer-events-none absolute left-4 top-4 flex items-center gap-2 border border-border bg-background/90 px-3 py-2 font-mono text-xs text-foreground backdrop-blur-sm">
            <Play className="h-3.5 w-3.5 text-primary" aria-hidden="true" />
            Load on play
          </div>
        </div>
      </section>

      <section aria-labelledby="faq-heading" className="border-y border-border bg-muted/15">
        <div className="mx-auto w-full max-w-6xl px-4 py-16 sm:px-6 sm:py-24 lg:px-8">
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Direct answers
          </p>
          <h2 id="faq-heading" className="mt-3 text-3xl font-semibold text-foreground">
            AllSource use-case questions
          </h2>
          <div className="mt-8 grid gap-px overflow-hidden border border-border bg-border lg:grid-cols-2">
            {useCaseFaqs.map((faq) => (
              <article key={faq.question} className="bg-background p-6 sm:p-8">
                <h3 className="font-semibold text-foreground">{faq.question}</h3>
                <p className="mt-3 text-sm leading-6 text-muted-foreground">{faq.answer}</p>
              </article>
            ))}
          </div>
        </div>
      </section>

      <section aria-labelledby="design-partners-heading" className="border-b border-border">
        <div className="mx-auto grid w-full max-w-6xl gap-6 px-4 py-10 sm:px-6 md:grid-cols-[1fr_auto] md:items-center lg:px-8">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Design-partner program
            </p>
            <h2
              id="design-partners-heading"
              className="mt-2 text-2xl font-semibold text-foreground"
            >
              Have one reproducible agent-memory failure?
            </h2>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-muted-foreground">
              Bring one production or serious pre-production flow. Work directly with AllSource
              engineering through first source-backed recall.
            </p>
          </div>
          <Link
            href="/design-partners"
            className={cn(buttonVariants({ variant: "outline" }), "w-full md:w-auto")}
          >
            Review design-partner program
            <ArrowRight className="ml-2 h-4 w-4" aria-hidden="true" />
          </Link>
        </div>
      </section>

      <section className="mx-auto w-full max-w-6xl px-4 py-16 sm:px-6 sm:py-24 lg:px-8">
        <div className="flex flex-col justify-between gap-8 border-l-2 border-primary pl-6 sm:flex-row sm:items-end sm:pl-8">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">Next event</p>
            <h2 className="mt-3 max-w-2xl text-3xl font-semibold text-foreground">
              Put one real workload through AllSource.
            </h2>
            <p className="mt-3 max-w-xl text-sm leading-6 text-muted-foreground">
              Use hosted services for managed operation, or run Apache-2.0 Core in your own stack.
            </p>
          </div>
          <div className="flex flex-col gap-3 sm:flex-row">
            <Link href="/signup" className={cn(buttonVariants({ variant: "default" }))}>
              Start hosted trial
              <ArrowRight className="ml-2 h-4 w-4" aria-hidden="true" />
            </Link>
            <Link
              href="https://github.com/all-source-os/all-source"
              className={cn(buttonVariants({ variant: "outline" }))}
            >
              View Core on GitHub
            </Link>
          </div>
        </div>
      </section>
    </main>
  );
}
