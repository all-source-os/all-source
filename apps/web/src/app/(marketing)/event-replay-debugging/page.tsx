import { Badge, buttonVariants, cn, Section } from "@allsource/ui";
import {
  ArrowRight,
  GitCompare,
  History,
  Layers3,
  RotateCcw,
  Search,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";
import { breadcrumbSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

export const metadata: Metadata = constructMetadata({
  title: "Event Replay Debugging for Production Systems",
  description:
    "Inspect ordered production events, compare historical state, analyze projection impact, and rebuild read models safely with AllSource Replay Studio and SDKs.",
  canonical: "/event-replay-debugging",
});

const workflow = [
  {
    number: "01",
    title: "Find one broken outcome",
    detail:
      "Start with entity, projection, tenant, or time window. Avoid replaying everything before you can state which derived result is wrong.",
    icon: Search,
  },
  {
    number: "02",
    title: "Read ordered source events",
    detail:
      "Inspect event type, entity, timestamp, version, and metadata in sequence. Current state alone cannot show which transition introduced divergence.",
    icon: History,
  },
  {
    number: "03",
    title: "Compare state across time",
    detail:
      "Reconstruct state before and after suspected event. This narrows investigation to first point where expected and actual state differ.",
    icon: GitCompare,
  },
  {
    number: "04",
    title: "Analyze replay impact",
    detail:
      "Preview event count, date range, event-type distribution, sampled entities, readiness checks, and warnings without changing source or projection state.",
    icon: ShieldCheck,
  },
  {
    number: "05",
    title: "Rebuild projection",
    detail:
      "Replay tenant-scoped events through corrected projection logic. Existing read model remains live until full rebuild succeeds and atomic replacement is ready.",
    icon: RotateCcw,
  },
  {
    number: "06",
    title: "Record result",
    detail:
      "Keep replay identifier, target, processed and failed counts, completion status, and resulting validation beside incident or migration evidence.",
    icon: Terminal,
  },
] as const;

const distinctions = [
  {
    title: "Timeline inspection",
    answer: "Read accepted events in order to explain what happened. No derived state changes.",
  },
  {
    title: "Point-in-time reconstruction",
    answer:
      "Fold events through a chosen timestamp to inspect what application state should have been then.",
  },
  {
    title: "Projection replay",
    answer:
      "Rebuild disposable read model from durable source events, commonly after projection logic or schema interpretation changes.",
  },
  {
    title: "Execution replay",
    answer:
      "Re-run application or agent behaviour. External calls, model responses, time, randomness, and side effects require separate capture or stubbing.",
  },
] as const;

const analysisOutput = [
  {
    field: "Scope",
    sdk: "total_events · sampled_events · analysis_scope",
    decision: "Whether review covers full tenant history or a sample.",
  },
  {
    field: "Time range",
    sdk: "first_event_at · last_event_at",
    decision: "Whether source window matches incident or migration boundary.",
  },
  {
    field: "Affected data",
    sdk: "event_type_distribution · sampled_entities",
    decision: "Which event types and entities deserve validation before rebuild.",
  },
  {
    field: "Readiness",
    sdk: "checks · warnings · ready_to_replay",
    decision: "Whether server-side invariants permit replay to start.",
  },
  {
    field: "Run evidence",
    sdk: "processed_events · failed_events · events_per_second",
    decision: "Whether completed run processed expected scope without hidden failures.",
  },
] as const;

const sdkExample = `import { AllSourceClient } from "@allsourcedev/client";

const client = new AllSourceClient({
  baseUrl: process.env.ALLSOURCE_URL!,
  apiKey: process.env.ALLSOURCE_API_KEY!,
});

const analysis = await client.analyzeProjectionReplay("event-count");

if (analysis.ready_to_replay && analysis.total_events > 0) {
  const run = await client.startProjectionReplay(analysis.projection_name);
  const progress = await client.getProjectionReplay(run.replay_id);
  console.log(progress.status, progress.progress_percentage);
}`;

function JsonLd({ value }: { value: object }) {
  return (
    <script
      type="application/ld+json"
      // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD requires a script tag; '<' is escaped before insertion
      dangerouslySetInnerHTML={{ __html: JSON.stringify(value).replace(/</g, "\\u003c") }}
    />
  );
}

export default function EventReplayDebuggingPage() {
  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "Use cases", path: "/use-cases" },
    { name: "Event replay debugging", path: "/event-replay-debugging" },
  ]);
  const article = {
    "@context": "https://schema.org",
    "@type": "TechArticle",
    headline: "Event replay debugging for production systems",
    description: metadata.description,
    url: "https://www.all-source.xyz/event-replay-debugging",
    mainEntityOfPage: "https://www.all-source.xyz/event-replay-debugging",
    datePublished: "2026-08-25",
    dateModified: "2026-08-25",
    inLanguage: "en",
    author: { "@id": "https://www.all-source.xyz/#organization" },
    publisher: { "@id": "https://www.all-source.xyz/#organization" },
    citation: [
      "https://www.martinfowler.com/eaaDev/EventSourcing.html",
      "https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing",
      "https://github.com/all-source-os/all-source/tree/main/sdks/typescript",
    ],
  };

  return (
    <div className="overflow-hidden">
      <JsonLd value={breadcrumb} />
      <JsonLd value={article} />

      <Section className="border-b border-border py-20 sm:py-28">
        <div className="mx-auto grid max-w-6xl gap-12 lg:grid-cols-[1.08fr_0.92fr] lg:items-end">
          <div>
            <Badge variant="outline" className="font-mono text-xs uppercase tracking-[0.18em]">
              Replay and production debugging
            </Badge>
            <h1 className="mt-6 max-w-4xl text-balance text-4xl font-semibold leading-[1.02] tracking-tight sm:text-6xl">
              Find first event where state went wrong.
            </h1>
            <p className="mt-6 max-w-2xl text-pretty text-lg leading-8 text-muted-foreground">
              AllSource keeps accepted state changes as ordered events. Inspect timeline, compare
              state across timestamps, preview replay impact, then rebuild one tenant projection
              without rewriting source history.
            </p>
            <div className="mt-8 flex flex-col gap-3 sm:flex-row">
              <Link href="/signup" className={cn(buttonVariants({ variant: "default" }))}>
                Start hosted trial
                <ArrowRight className="ml-2 h-4 w-4" aria-hidden="true" />
              </Link>
              <Link
                href="/dashboard/tools/replay"
                className={cn(buttonVariants({ variant: "outline" }))}
              >
                Open Replay Studio
              </Link>
            </div>
          </div>

          <figure className="border border-border bg-card">
            <div className="flex items-center justify-between border-b border-border px-5 py-3">
              <span className="font-mono text-xs uppercase tracking-[0.18em] text-muted-foreground">
                Projection divergence
              </span>
              <GitCompare className="h-4 w-4 text-primary" aria-hidden="true" />
            </div>
            <ol className="divide-y divide-border font-mono text-sm">
              <li className="grid grid-cols-[5rem_1fr_auto] gap-3 px-5 py-4">
                <span className="text-muted-foreground">10:01:04</span>
                <span>checkout.started</span>
                <span className="text-emerald-500">state valid</span>
              </li>
              <li className="grid grid-cols-[5rem_1fr_auto] gap-3 border-l-2 border-l-destructive px-5 py-4">
                <span className="text-muted-foreground">10:01:12</span>
                <span>payment.authorized</span>
                <span className="text-destructive">first diff</span>
              </li>
              <li className="grid grid-cols-[5rem_1fr_auto] gap-3 px-5 py-4">
                <span className="text-muted-foreground">10:01:15</span>
                <span>inventory.rejected</span>
                <span className="text-muted-foreground">downstream</span>
              </li>
            </ol>
            <figcaption className="border-t border-primary/30 bg-primary/5 p-5 text-sm leading-6">
              Rebuild corrected read model from same source sequence; current projection remains
              available until successful replacement.
            </figcaption>
          </figure>
        </div>
      </Section>

      <Section className="py-16 sm:py-24">
        <div className="mx-auto max-w-3xl">
          <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
            Direct answer
          </p>
          <h2 className="mt-4 text-3xl font-semibold tracking-tight sm:text-4xl">
            What is event replay debugging?
          </h2>
          <div className="mt-6 space-y-5 text-base leading-8 text-muted-foreground">
            <p>
              Event replay debugging uses the durable sequence of domain events to reconstruct how
              state changed. Instead of starting with an incorrect current row and guessing, an
              engineer reads events in order, derives state at selected boundaries, and identifies
              the first transition where the actual result diverges from the expected result.
            </p>
            <p>
              Projection replay is a narrower operation. It runs existing source events through
              read-model logic again. This can rebuild a projection after a code correction, add
              historical data to a new view, or validate a changed interpretation. It should not
              mutate accepted source events. Incorrect facts require explicit correction or
              compensating events, not silent history edits.
            </p>
          </div>
        </div>
      </Section>

      <Section className="border-y border-border bg-muted/15 py-16 sm:py-24">
        <div className="mx-auto max-w-6xl">
          <div className="max-w-3xl">
            <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
              Investigation workflow
            </p>
            <h2 className="mt-4 text-3xl font-semibold tracking-tight sm:text-4xl">
              Move from symptom to bounded replay.
            </h2>
            <p className="mt-4 text-lg leading-8 text-muted-foreground">
              Do not start by replaying everything. Bound the evidence, inspect impact, then rebuild
              one disposable view.
            </p>
          </div>
          <ol className="mt-10 grid gap-px overflow-hidden border border-border bg-border md:grid-cols-2 lg:grid-cols-3">
            {workflow.map((step) => {
              const Icon = step.icon;
              return (
                <li key={step.number} className="bg-background p-6">
                  <div className="flex items-center justify-between">
                    <span className="font-mono text-xs text-muted-foreground">{step.number}</span>
                    <Icon className="h-5 w-5 text-primary" aria-hidden="true" />
                  </div>
                  <h3 className="mt-8 text-lg font-semibold">{step.title}</h3>
                  <p className="mt-3 text-sm leading-6 text-muted-foreground">{step.detail}</p>
                </li>
              );
            })}
          </ol>
        </div>
      </Section>

      <Section className="border-b border-border py-16 sm:py-24">
        <div className="mx-auto max-w-6xl">
          <div className="max-w-3xl">
            <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
              Concrete replay contract
            </p>
            <h2 className="mt-4 text-3xl font-semibold tracking-tight sm:text-4xl">
              What does replay analysis return?
            </h2>
            <p className="mt-5 text-lg leading-8 text-muted-foreground">
              TypeScript and Rust SDKs expose the same tenant-scoped analysis fields before a replay
              starts. Those fields turn “rebuild it” into a reviewable decision with a bounded
              source range, affected entities, server checks, warnings, and run evidence.
            </p>
          </div>
          <div className="mt-10 hidden overflow-x-auto border border-border sm:block">
            <table className="w-full min-w-[48rem] border-collapse text-left">
              <thead className="bg-muted/40 font-mono text-xs uppercase tracking-[0.12em] text-muted-foreground">
                <tr>
                  <th className="border-b border-border px-5 py-4 font-medium" scope="col">
                    Review
                  </th>
                  <th className="border-b border-border px-5 py-4 font-medium" scope="col">
                    SDK fields
                  </th>
                  <th className="border-b border-border px-5 py-4 font-medium" scope="col">
                    Operator decision
                  </th>
                </tr>
              </thead>
              <tbody>
                {analysisOutput.map((row) => (
                  <tr key={row.field} className="border-b border-border last:border-b-0">
                    <th className="px-5 py-4 font-semibold" scope="row">
                      {row.field}
                    </th>
                    <td className="px-5 py-4 font-mono text-sm text-primary">{row.sdk}</td>
                    <td className="px-5 py-4 text-sm leading-6 text-muted-foreground">
                      {row.decision}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <dl className="mt-8 grid gap-3 sm:hidden">
            {analysisOutput.map((row) => (
              <div key={row.field} className="border border-border bg-card p-5">
                <dt className="font-semibold">{row.field}</dt>
                <dd className="mt-3 font-mono text-sm leading-6 text-primary">{row.sdk}</dd>
                <dd className="mt-3 text-sm leading-6 text-muted-foreground">{row.decision}</dd>
              </div>
            ))}
          </dl>
        </div>
      </Section>

      <Section className="py-16 sm:py-24">
        <div className="mx-auto grid max-w-6xl gap-12 lg:grid-cols-[0.9fr_1.1fr]">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
              Know which replay you mean
            </p>
            <h2 className="mt-4 text-3xl font-semibold tracking-tight">
              Four operations, different risks.
            </h2>
            <p className="mt-5 leading-7 text-muted-foreground">
              Event stores support history and derived-state reconstruction. They do not
              automatically capture every nondeterministic input needed to re-execute application or
              LLM behaviour exactly.
            </p>
            <Link
              href="/platform/event-sourcing"
              className="mt-6 inline-flex items-center font-medium text-primary underline underline-offset-4"
            >
              Read event-store architecture
              <ArrowRight className="ml-2 h-4 w-4" aria-hidden="true" />
            </Link>
          </div>
          <dl className="border border-border bg-card">
            {distinctions.map((item) => (
              <div
                key={item.title}
                className="grid gap-2 border-b border-border p-5 last:border-b-0 sm:grid-cols-[12rem_1fr]"
              >
                <dt className="font-semibold text-foreground">{item.title}</dt>
                <dd className="text-sm leading-6 text-muted-foreground">{item.answer}</dd>
              </div>
            ))}
          </dl>
        </div>
      </Section>

      <Section className="border-y border-border bg-[#090b12] py-16 text-white sm:py-24">
        <div className="mx-auto grid max-w-6xl gap-10 lg:grid-cols-[0.75fr_1.25fr] lg:items-start">
          <div>
            <Badge variant="outline" className="border-white/25 text-white">
              TypeScript SDK
            </Badge>
            <h2 className="mt-5 text-3xl font-semibold tracking-tight">
              Analyze before starting replay.
            </h2>
            <p className="mt-5 leading-7 text-white/70">
              Query Service keeps replay tenant-scoped. Analysis returns event volume, sample
              coverage, affected event types and entities, readiness checks, and warnings without
              modifying source or projection state.
            </p>
            <p className="mt-5 leading-7 text-white/70">
              Start only after checks pass. Follow replay identifier for status and progress. Cancel
              running rebuild if assumptions change; successful replacement remains separate from
              immutable event history.
            </p>
          </div>
          <pre className="overflow-x-auto border border-white/15 bg-black/40 p-6 text-sm leading-7 text-white/85">
            <code>{sdkExample}</code>
          </pre>
        </div>
      </Section>

      <Section className="py-16 sm:py-24">
        <div className="mx-auto grid max-w-6xl gap-10 lg:grid-cols-2">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
              Safe operating boundary
            </p>
            <h2 className="mt-4 text-3xl font-semibold tracking-tight">
              Source survives failed rebuild.
            </h2>
            <div className="mt-6 space-y-4 leading-7 text-muted-foreground">
              <p>
                AllSource Core remains the durable source of truth through WAL and Parquet. Query
                Service owns tenant-facing projection compute and replay jobs. Projection state is
                rebuildable; event history is not swapped or deleted by projection replay.
              </p>
              <p>
                Replay Studio requires impact analysis before starting atomic rebuild. Current read
                model stays live during build. Failed or cancelled run does not replace it. This
                protects availability, but the application team still must validate projector
                determinism, schema compatibility, and expected result.
              </p>
            </div>
          </div>
          <div className="border border-border bg-card p-6">
            <Layers3 className="h-6 w-6 text-primary" aria-hidden="true" />
            <h3 className="mt-5 text-xl font-semibold">What to verify after replay</h3>
            <ul className="mt-5 space-y-3 text-sm leading-6 text-muted-foreground">
              <li>Processed event count matches analyzed scope.</li>
              <li>Failed event count is zero or explicitly understood.</li>
              <li>Known entities match expected state at selected boundaries.</li>
              <li>New projection answers realtime, HTTP, and analytics reads as designed.</li>
              <li>Incident record links replay run and validation evidence, never raw secrets.</li>
            </ul>
          </div>
        </div>
      </Section>

      <Section className="border-t border-border py-12">
        <div className="mx-auto grid max-w-6xl gap-5 md:grid-cols-[14rem_1fr]">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
              Primary references
            </p>
            <h2 className="mt-3 text-xl font-semibold">Verify the model and SDK contract.</h2>
          </div>
          <ul className="divide-y divide-border border-y border-border">
            <li>
              <Link
                href="https://learn.microsoft.com/en-us/azure/architecture/patterns/event-sourcing"
                className="flex min-h-14 items-center py-3 font-medium text-primary underline underline-offset-4"
              >
                Microsoft Azure Architecture Center: Event Sourcing pattern
              </Link>
            </li>
            <li>
              <Link
                href="https://martinfowler.com/eaaDev/EventSourcing.html"
                className="flex min-h-14 items-center py-3 font-medium text-primary underline underline-offset-4"
              >
                Martin Fowler: Event Sourcing
              </Link>
            </li>
            <li>
              <Link
                href="https://github.com/all-source-os/all-source/blob/main/sdks/typescript/README.md#projection-replay-analysis"
                className="flex min-h-14 items-center py-3 font-medium text-primary underline underline-offset-4"
              >
                AllSource TypeScript SDK: projection replay analysis
              </Link>
            </li>
          </ul>
        </div>
      </Section>

      <Section className="border-t border-border py-16 text-center sm:py-24">
        <h2 className="text-3xl font-semibold tracking-tight">
          Debug from history, not guesswork.
        </h2>
        <p className="mx-auto mt-5 max-w-2xl leading-7 text-muted-foreground">
          Start hosted trial, inspect API and SDK workflow, or self-host Apache-2.0 Core. Replay
          features matter when source history, projection rebuilds, and point-in-time evidence are
          part of normal product operation—not when current state alone is enough.
        </p>
        <div className="mt-8 flex flex-col justify-center gap-3 sm:flex-row">
          <Link href="/pricing" className={cn(buttonVariants({ variant: "default" }))}>
            See hosted pricing
          </Link>
          <Link href="/docs/api" className={cn(buttonVariants({ variant: "outline" }))}>
            Read API docs
          </Link>
          <Link
            href="https://github.com/all-source-os/all-source"
            className={cn(buttonVariants({ variant: "outline" }))}
          >
            View source
          </Link>
        </div>
      </Section>
    </div>
  );
}
