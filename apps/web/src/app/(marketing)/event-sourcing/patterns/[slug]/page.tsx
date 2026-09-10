import { Badge, buttonVariants, cn } from "@allsource/ui";
import {
  AlertTriangle,
  ArrowLeft,
  ArrowRight,
  CheckCircle2,
  ExternalLink,
  TerminalSquare,
} from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import {
  eventSourcingPatterns,
  getEventSourcingPattern,
  getPatternWordCount,
} from "@/lib/event-sourcing-patterns";
import { breadcrumbSchema, techArticleSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

type PatternPageProps = {
  params: Promise<{ slug: string }>;
};

export const dynamicParams = false;

export function generateStaticParams() {
  return eventSourcingPatterns.map((pattern) => ({ slug: pattern.slug }));
}

export async function generateMetadata({ params }: PatternPageProps): Promise<Metadata> {
  const { slug } = await params;
  const pattern = getEventSourcingPattern(slug);
  if (!pattern) return {};

  return constructMetadata({
    title: pattern.title,
    description: pattern.description,
    canonical: `/event-sourcing/patterns/${pattern.slug}`,
    type: "article",
    publishedTime: "2026-09-10",
    modifiedTime: "2026-09-10",
    authors: ["Decebal Dobrica"],
    section: "Event sourcing patterns",
  });
}

function JsonLd({ value }: { value: object }) {
  return (
    <script
      type="application/ld+json"
      // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD requires script content; '<' is escaped before insertion
      dangerouslySetInnerHTML={{ __html: JSON.stringify(value).replace(/</g, "\\u003c") }}
    />
  );
}

export default async function EventSourcingPatternPage({ params }: PatternPageProps) {
  const { slug } = await params;
  const pattern = getEventSourcingPattern(slug);
  if (!pattern) notFound();

  const wordCount = getPatternWordCount(pattern);
  const relatedPatterns = pattern.related
    .map((relatedSlug) => getEventSourcingPattern(relatedSlug))
    .filter((related) => related !== undefined);

  return (
    <article className="mx-auto w-full max-w-7xl px-4 py-20 sm:px-6 sm:py-24 lg:px-8">
      <JsonLd
        value={breadcrumbSchema([
          { name: "Home", path: "/" },
          { name: "Event sourcing patterns", path: "/event-sourcing/patterns" },
          { name: pattern.shortTitle, path: `/event-sourcing/patterns/${pattern.slug}` },
        ])}
      />
      <JsonLd value={techArticleSchema(pattern, wordCount)} />

      <header className="grid gap-10 border-b border-border pb-14 lg:grid-cols-[minmax(0,1fr)_18rem] lg:items-end">
        <div>
          <Link
            href="/event-sourcing/patterns"
            className="inline-flex min-h-11 items-center gap-2 text-sm text-muted-foreground underline-offset-4 hover:text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <ArrowLeft className="h-4 w-4" aria-hidden="true" />
            All event-sourcing patterns
          </Link>
          <div className="mt-7 flex flex-wrap items-center gap-3">
            <Badge variant="outline" className="font-mono text-xs uppercase tracking-[0.18em]">
              Production pattern
            </Badge>
            <span className="font-mono text-xs text-muted-foreground">
              {wordCount.toLocaleString("en-GB")} words · verified 10 September 2026
            </span>
          </div>
          <h1 className="mt-6 max-w-5xl text-balance text-4xl font-semibold leading-tight tracking-tight text-foreground sm:text-6xl">
            {pattern.title}
          </h1>
          <p className="mt-7 max-w-4xl text-pretty text-xl leading-9 text-foreground">
            {pattern.directAnswer}
          </p>
        </div>
        <aside className="border-l-2 border-primary pl-5 text-sm leading-6 text-muted-foreground">
          <p className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
            AllSource model
          </p>
          <p className="mt-3">
            Core stores immutable facts. Query Service builds tenant-facing read paths. Prime
            derives agent memory from same event history.
          </p>
        </aside>
      </header>

      <div className="grid gap-14 py-16 lg:grid-cols-[minmax(0,1fr)_18rem] lg:items-start">
        <div className="space-y-16">
          <section aria-labelledby="problem-heading">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">Problem</p>
            <h2 id="problem-heading" className="mt-3 text-3xl font-semibold text-foreground">
              Why this pattern exists
            </h2>
            <div className="mt-6 space-y-5 text-base leading-8 text-muted-foreground">
              {pattern.problem.map((paragraph) => (
                <p key={paragraph}>{paragraph}</p>
              ))}
            </div>
          </section>

          <section aria-labelledby="decisions-heading">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Design decisions
            </p>
            <h2 id="decisions-heading" className="mt-3 text-3xl font-semibold text-foreground">
              Make boundaries explicit
            </h2>
            <ol className="mt-8 grid gap-px overflow-hidden border border-border bg-border">
              {pattern.decisions.map((decision, index) => (
                <li
                  key={decision.title}
                  className="grid gap-4 bg-background p-6 sm:grid-cols-[3rem_1fr]"
                >
                  <span className="font-mono text-sm text-primary">
                    {String(index + 1).padStart(2, "0")}
                  </span>
                  <div>
                    <h3 className="text-lg font-semibold text-foreground">{decision.title}</h3>
                    <p className="mt-2 leading-7 text-muted-foreground">{decision.detail}</p>
                  </div>
                </li>
              ))}
            </ol>
          </section>

          <section aria-labelledby="implementation-heading">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              AllSource implementation
            </p>
            <h2 id="implementation-heading" className="mt-3 text-3xl font-semibold text-foreground">
              Apply pattern to durable Core history
            </h2>
            <div className="mt-6 space-y-5 text-base leading-8 text-muted-foreground">
              {pattern.implementation.map((paragraph) => (
                <p key={paragraph}>{paragraph}</p>
              ))}
            </div>

            <figure className="mt-8 overflow-hidden border border-border bg-card">
              <figcaption className="flex items-center gap-2 border-b border-border px-5 py-3 font-mono text-xs uppercase tracking-[0.14em] text-muted-foreground">
                <TerminalSquare className="h-4 w-4 text-primary" aria-hidden="true" />
                {pattern.exampleTitle}
              </figcaption>
              <pre className="overflow-x-auto p-5 text-sm leading-7 text-foreground">
                <code>{pattern.example}</code>
              </pre>
            </figure>
          </section>

          <section aria-labelledby="failure-heading">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Failure modes
            </p>
            <h2 id="failure-heading" className="mt-3 text-3xl font-semibold text-foreground">
              Detect weak implementations early
            </h2>
            <div className="mt-8 grid gap-4">
              {pattern.failureModes.map((failure) => (
                <article key={failure.symptom} className="border border-border bg-card p-6">
                  <div className="flex gap-4">
                    <AlertTriangle
                      className="mt-0.5 h-5 w-5 shrink-0 text-primary"
                      aria-hidden="true"
                    />
                    <div>
                      <h3 className="font-semibold text-foreground">{failure.symptom}</h3>
                      <p className="mt-2 text-sm leading-6 text-muted-foreground">
                        <span className="font-medium text-foreground">Fix:</span> {failure.fix}
                      </p>
                    </div>
                  </div>
                </article>
              ))}
            </div>
          </section>

          <section aria-labelledby="checklist-heading">
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Production checklist
            </p>
            <h2 id="checklist-heading" className="mt-3 text-3xl font-semibold text-foreground">
              Ready when each statement is true
            </h2>
            <ul className="mt-8 grid gap-px overflow-hidden border border-border bg-border sm:grid-cols-2">
              {pattern.checklist.map((item) => (
                <li
                  key={item}
                  className="flex gap-3 bg-background p-5 text-sm leading-6 text-foreground"
                >
                  <CheckCircle2
                    className="mt-0.5 h-4 w-4 shrink-0 text-primary"
                    aria-hidden="true"
                  />
                  {item}
                </li>
              ))}
            </ul>
          </section>
        </div>

        <aside className="space-y-8 lg:sticky lg:top-28">
          <div className="border border-border bg-card p-5">
            <h2 className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
              Authoritative references
            </h2>
            <ul className="mt-4 space-y-4">
              {pattern.references.map((reference) => (
                <li key={reference.href}>
                  <Link
                    href={reference.href}
                    className="inline-flex items-start gap-2 text-sm leading-6 text-muted-foreground underline-offset-4 hover:text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    {reference.label}
                    <ExternalLink className="mt-1 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                  </Link>
                </li>
              ))}
            </ul>
          </div>

          <div className="border border-border p-5">
            <h2 className="font-mono text-xs uppercase tracking-[0.18em] text-primary">
              Product paths
            </h2>
            <nav className="mt-4 flex flex-col gap-2" aria-label="AllSource product paths">
              {(
                [
                  ["Event store platform", "/platform/event-sourcing"],
                  ["API reference", "/docs/api"],
                  ["Architecture", "/architecture"],
                  ["Compare EventStoreDB", "/compare/eventstoredb"],
                ] as const
              ).map(([label, href]) => (
                <Link
                  key={href}
                  href={href}
                  className="inline-flex min-h-10 items-center justify-between gap-2 text-sm text-muted-foreground hover:text-primary focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  {label}
                  <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
                </Link>
              ))}
            </nav>
          </div>
        </aside>
      </div>

      <section aria-labelledby="related-heading" className="border-t border-border py-16">
        <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
          Related patterns
        </p>
        <h2 id="related-heading" className="mt-3 text-3xl font-semibold text-foreground">
          Continue through adjacent decisions
        </h2>
        <div className="mt-8 grid gap-px overflow-hidden border border-border bg-border lg:grid-cols-3">
          {relatedPatterns.map((related) => (
            <Link
              key={related.slug}
              href={`/event-sourcing/patterns/${related.slug}`}
              className="group bg-background p-6 transition-colors hover:bg-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
            >
              <h3 className="text-lg font-semibold text-foreground group-hover:text-primary">
                {related.shortTitle}
              </h3>
              <p className="mt-3 text-sm leading-6 text-muted-foreground">{related.directAnswer}</p>
              <span className="mt-5 inline-flex items-center gap-2 font-mono text-xs font-semibold text-primary">
                Read next <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
              </span>
            </Link>
          ))}
        </div>
      </section>

      <section className="flex flex-col justify-between gap-6 border-t border-border py-16 sm:flex-row sm:items-end">
        <div>
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
            Store history once
          </p>
          <h2 className="mt-3 max-w-2xl text-3xl font-semibold text-foreground">
            Rebuild every useful view from durable events.
          </h2>
        </div>
        <div className="flex flex-col gap-3 sm:flex-row">
          <Link href="/signup" className={cn(buttonVariants({ variant: "default" }), "gap-2")}>
            Start 14-day trial <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </Link>
          <Link
            href="https://github.com/all-source-os/all-source"
            className={cn(buttonVariants({ variant: "outline" }))}
          >
            Self-host Core
          </Link>
        </div>
      </section>
    </article>
  );
}
