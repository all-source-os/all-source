import { Badge, buttonVariants, cn } from "@allsource/ui";
import { ArrowRight, Brain, Cable, Cloud, Database, ExternalLink, Radio } from "lucide-react";
import Link from "next/link";
import {
  allsourceIdentity,
  type ProductVerticalId,
  productIdentityFaqs,
  productVerticals,
} from "@/lib/product-verticals";
import { breadcrumbSchema, faqPageSchema, productVerticalListSchema } from "@/lib/structured-data";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "What Is AllSource? Event Store Database Explained",
  description:
    "AllSource is an event store database for immutable streams, replay, projections, and temporal queries. See where Core, Query Service, and Prime fit.",
  canonical: "/what-is-allsource",
});

const iconByVertical: Record<ProductVerticalId, typeof Database> = {
  core: Database,
  query: Radio,
  prime: Brain,
  hosted: Cloud,
  mcp: Cable,
};

function JsonLd({ value }: { value: object }) {
  return (
    <script
      type="application/ld+json"
      // biome-ignore lint/security/noDangerouslySetInnerHtml: JSON-LD requires a script tag; '<' is escaped before insertion
      dangerouslySetInnerHTML={{ __html: JSON.stringify(value).replace(/</g, "\\u003c") }}
    />
  );
}

export default function WhatIsAllSourcePage() {
  const breadcrumb = breadcrumbSchema([
    { name: "Home", path: "/" },
    { name: "What is AllSource?", path: "/what-is-allsource" },
  ]);

  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-24 sm:px-6 lg:px-8">
      <JsonLd value={breadcrumb} />
      <JsonLd value={faqPageSchema(productIdentityFaqs)} />
      <JsonLd value={productVerticalListSchema(productVerticals)} />

      <header className="border-b border-border pb-12">
        <div className="mb-6 flex flex-wrap items-center gap-3">
          <Badge variant="outline" className="font-mono text-xs uppercase tracking-[0.18em]">
            Canonical entity answer
          </Badge>
          <span className="font-mono text-xs text-muted-foreground">Verified 14 August 2026</span>
        </div>
        <h1 className="max-w-4xl text-balance text-4xl font-semibold leading-tight tracking-tight text-foreground sm:text-6xl">
          What is AllSource Event Store?
        </h1>
        <p className="mt-7 max-w-4xl text-pretty text-xl leading-9 text-foreground sm:text-2xl">
          {allsourceIdentity.directAnswer}
        </p>
        <p className="mt-5 max-w-4xl border-l-2 border-primary pl-4 text-base leading-7 text-muted-foreground">
          {allsourceIdentity.disambiguation}
        </p>
      </header>

      <section aria-labelledby="product-map-heading" className="py-16">
        <div className="grid gap-8 lg:grid-cols-[0.72fr_1.28fr] lg:items-start">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">Product map</p>
            <h2 id="product-map-heading" className="mt-3 text-3xl font-semibold text-foreground">
              One event store. Five precise layers.
            </h2>
            <p className="mt-4 text-base leading-7 text-muted-foreground">
              “AllSource” names the platform. Use the component name when describing a specific job.
              Core stores. Query Service reads. Prime remembers. Hosted services operate. MCP
              connectors expose tools.
            </p>
          </div>

          <figure className="overflow-hidden border border-border bg-card">
            <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-stretch">
              <div className="border-b border-r border-border p-5">
                <p className="font-mono text-xs uppercase tracking-widest text-muted-foreground">
                  Applications
                </p>
                <p className="mt-2 text-sm text-foreground">HTTP · WebSocket · SDK</p>
              </div>
              <div className="grid place-items-center border-b border-border px-4 font-mono text-primary">
                →
              </div>
              <div className="border-b border-l border-border p-5">
                <p className="font-mono text-xs uppercase tracking-widest text-muted-foreground">
                  Query Service
                </p>
                <p className="mt-2 text-sm text-foreground">HTTP · realtime · analytics · views</p>
              </div>

              <div className="border-r border-border p-5">
                <p className="font-mono text-xs uppercase tracking-widest text-muted-foreground">
                  Control + agent layers
                </p>
                <p className="mt-2 text-sm text-foreground">Auth · billing · Prime · MCP</p>
              </div>
              <div className="grid place-items-center px-4 font-mono text-primary">→</div>
              <div className="border-l border-border bg-primary/5 p-5">
                <p className="font-mono text-xs uppercase tracking-widest text-primary">
                  AllSource Core
                </p>
                <p className="mt-2 text-sm text-foreground">Immutable events · replay · state</p>
              </div>
            </div>
            <figcaption className="border-t border-border px-5 py-3 text-xs leading-5 text-muted-foreground">
              Core is durable record. Query Service exposes distinct HTTP, realtime, analytics, and
              projection reads from that record. Prime derives agent context. Connectors transport
              tool calls; none becomes another database.
            </figcaption>
          </figure>
        </div>
      </section>

      <section aria-labelledby="verticals-heading" className="border-t border-border py-16">
        <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
          Product boundaries
        </p>
        <h2 id="verticals-heading" className="mt-3 text-3xl font-semibold text-foreground">
          Name the layer you mean
        </h2>
        <div className="mt-9 grid gap-px overflow-hidden border border-border bg-border lg:grid-cols-2">
          {productVerticals.map((vertical) => {
            const Icon = iconByVertical[vertical.id];
            return (
              <article key={vertical.id} className="bg-background p-6 sm:p-8">
                <div className="flex items-start justify-between gap-6">
                  <div>
                    <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
                      {vertical.role}
                    </p>
                    <h3 className="mt-2 text-2xl font-semibold text-foreground">{vertical.name}</h3>
                  </div>
                  <Icon className="h-6 w-6 text-muted-foreground" aria-hidden="true" />
                </div>
                <p className="mt-5 leading-7 text-foreground">{vertical.directAnswer}</p>
                <dl className="mt-6 space-y-4 text-sm leading-6">
                  <div>
                    <dt className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
                      Holds
                    </dt>
                    <dd className="mt-1 text-foreground">{vertical.stores}</dd>
                  </div>
                  <div>
                    <dt className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
                      Use when
                    </dt>
                    <dd className="mt-1 text-foreground">{vertical.useWhen}</dd>
                  </div>
                  <div>
                    <dt className="font-mono text-xs uppercase tracking-wider text-muted-foreground">
                      Boundary
                    </dt>
                    <dd className="mt-1 text-muted-foreground">{vertical.notThis}</dd>
                  </div>
                </dl>
                <Link
                  href={vertical.path}
                  className="mt-6 inline-flex items-center gap-2 text-sm font-medium text-primary underline-offset-4 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                  Read {vertical.name.replace("AllSource ", "")}
                  <ArrowRight className="h-4 w-4" aria-hidden="true" />
                </Link>
              </article>
            );
          })}
        </div>
      </section>

      <section aria-labelledby="disambiguation-heading" className="border-t border-border py-16">
        <div className="grid gap-8 lg:grid-cols-[0.72fr_1.28fr]">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Name disambiguation
            </p>
            <h2 id="disambiguation-heading" className="mt-3 text-3xl font-semibold text-foreground">
              Which AllSource?
            </h2>
          </div>
          <div className="divide-y divide-border border-y border-border">
            {[
              [
                "AllSource Event Store",
                "Developer infrastructure at all-source.xyz: Core, Query Service, Prime, hosted services, and MCP connectors.",
              ],
              [
                "ArcGIS AllSource",
                "Esri desktop intelligence-analysis and geospatial software. Unrelated product.",
              ],
              [
                "All-source intelligence",
                "Intelligence discipline combining multiple collection sources. Not this software.",
              ],
              [
                "Other Allsource companies",
                "Audience data, logistics, procurement, and contracting businesses using similar names. Unrelated.",
              ],
            ].map(([name, description]) => (
              <div key={name} className="grid gap-2 py-5 sm:grid-cols-[13rem_1fr] sm:gap-6">
                <h3 className="font-medium text-foreground">{name}</h3>
                <p className="text-sm leading-6 text-muted-foreground">{description}</p>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section aria-labelledby="faq-heading" className="border-t border-border py-16">
        <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">Direct answers</p>
        <h2 id="faq-heading" className="mt-3 text-3xl font-semibold text-foreground">
          Questions answer engines should get right
        </h2>
        <div className="mt-8 grid gap-px overflow-hidden border border-border bg-border lg:grid-cols-2">
          {productIdentityFaqs.map((faq) => (
            <article key={faq.question} className="bg-background p-6">
              <h3 className="font-semibold text-foreground">{faq.question}</h3>
              <p className="mt-3 text-sm leading-6 text-muted-foreground">{faq.answer}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="border-t border-border py-16">
        <div className="flex flex-col justify-between gap-6 sm:flex-row sm:items-end">
          <div>
            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary">
              Choose a route
            </p>
            <h2 className="mt-3 max-w-2xl text-3xl font-semibold text-foreground">
              Inspect source, run Core, or connect one agent.
            </h2>
          </div>
          <div className="flex flex-col gap-3 sm:flex-row">
            <Link href="/docs" className={cn(buttonVariants({ variant: "default" }), "gap-2")}>
              Read documentation <ArrowRight className="h-4 w-4" aria-hidden="true" />
            </Link>
            <Link
              href="https://github.com/all-source-os/all-source"
              className={cn(buttonVariants({ variant: "outline" }), "gap-2")}
            >
              Inspect repository <ExternalLink className="h-4 w-4" aria-hidden="true" />
            </Link>
          </div>
        </div>
      </section>
    </div>
  );
}
