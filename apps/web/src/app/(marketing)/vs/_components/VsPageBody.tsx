"use client";

import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  buttonVariants,
  cn,
  Section,
} from "@allsource/ui";
import { Check, ChevronRight } from "lucide-react";
import Link from "next/link";
import { staticMotion as motion } from "@/components/ui/static-motion";
import type { Competitor } from "../_data/competitors";
import { ComparisonTable } from "./ComparisonTable";

/**
 * Shared body for every `/vs/<slug>` page. The slug `page.tsx` files stay thin
 * — they supply metadata + JSON-LD and render this with the competitor data.
 */
export function VsPageBody({ competitor }: { competitor: Competitor }) {
  return (
    <div className="relative overflow-hidden">
      {/* Hero: H1 + one-line verdict + CTA to /pricing */}
      <Section className="relative pt-24 pb-12 text-center">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6 }}
        >
          <h1 className="text-4xl font-bold tracking-tight sm:text-5xl">
            AllSource vs {competitor.name}
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-lg text-muted-foreground">
            {competitor.verdict}
          </p>
          <div className="mt-8 flex flex-wrap items-center justify-center gap-4">
            <Link href="/pricing" className={cn(buttonVariants({ variant: "default" }))}>
              Start Indie <ChevronRight className="ml-1 h-4 w-4" />
            </Link>
            <Link href="/pricing" className={cn(buttonVariants({ variant: "outline" }))}>
              {competitor.selfHostLabel ?? "Self-host (free, Apache-2.0)"}
            </Link>
          </div>
        </motion.div>
      </Section>

      {/* Optional "different category" framing — only rendered when the data
            map supplies it (stoolap). Absent for mem0/letta/zep. */}
      {competitor.category && (
        <Section className="pb-4">
          <div className="mx-auto max-w-3xl rounded-xl border border-border bg-muted/20 p-6">
            <h2 className="mb-2 text-lg font-semibold">{competitor.category.heading}</h2>
            <p className="text-sm text-muted-foreground">{competitor.category.body}</p>
          </div>
        </Section>
      )}

      {/* Comparison table */}
      <Section className="pb-4">
        <ComparisonTable competitorName={competitor.name} rows={competitor.rows} />
        <p className="mx-auto mt-4 max-w-2xl text-center text-xs text-muted-foreground">
          {competitor.tableNote ?? (
            <>
              AllSource figures are from our own benchmarks and documentation. Cells marked
              &ldquo;unknown&rdquo; or &ldquo;varies&rdquo; are values we do not publish for{" "}
              {competitor.name} rather than estimate. Based on public documentation; corrections
              welcome.
            </>
          )}
        </p>
      </Section>

      <Section className="pb-12">
        <div className="mx-auto max-w-3xl border-y border-border py-6">
          <h2 className="font-mono text-xs uppercase tracking-[0.18em] text-muted-foreground">
            Primary sources checked
          </h2>
          <ul className="mt-4 flex flex-wrap gap-x-6 gap-y-3 text-sm">
            {competitor.sources.map((source) => (
              <li key={source.href}>
                <Link
                  href={source.href}
                  target={source.href.startsWith("http") ? "_blank" : undefined}
                  rel={source.href.startsWith("http") ? "noopener noreferrer" : undefined}
                  className="inline-flex min-h-12 items-center text-primary underline underline-offset-4"
                >
                  {source.label} ↗
                </Link>
              </li>
            ))}
          </ul>
        </div>
      </Section>

      {/* When to pick which */}
      <Section className="pb-12">
        <div className="mx-auto grid max-w-4xl gap-6 md:grid-cols-2">
          <div className="rounded-xl border p-6">
            <h2 className="mb-3 text-lg font-semibold">Pick AllSource if…</h2>
            <ul className="space-y-2 text-sm">
              {competitor.pickAllsource.map((item) => (
                <li key={item} className="flex items-start gap-2">
                  <Check className="mt-0.5 h-4 w-4 shrink-0 text-primary" />
                  {item}
                </li>
              ))}
            </ul>
          </div>
          <div className="rounded-xl border p-6">
            <h2 className="mb-3 text-lg font-semibold">Pick {competitor.name} if…</h2>
            <ul className="space-y-2 text-sm">
              {competitor.pickCompetitor.map((item) => (
                <li key={item} className="flex items-start gap-2">
                  <Check className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />
                  {item}
                </li>
              ))}
            </ul>
          </div>
        </div>
      </Section>

      {/* FAQ — answers the obvious "is X better than AllSource?" intent */}
      <Section className="pb-12">
        <div className="mx-auto max-w-3xl">
          <h2 className="mb-6 text-center text-2xl font-bold">
            AllSource vs {competitor.name}: FAQ
          </h2>
          <Accordion type="single" collapsible className="flex w-full flex-col space-y-2">
            {competitor.faqs.map((faq) => (
              <AccordionItem
                key={faq.question}
                value={faq.question}
                className="w-full overflow-hidden rounded-lg border"
              >
                <AccordionTrigger className="px-4 text-left">{faq.question}</AccordionTrigger>
                <AccordionContent className="px-4">{faq.answer}</AccordionContent>
              </AccordionItem>
            ))}
          </Accordion>
        </div>
      </Section>

      {/* CTA + internal links to the pillar and other comparisons */}
      <Section className="pb-24">
        <div className="mx-auto max-w-2xl rounded-xl border bg-muted/20 p-8 text-center">
          <h2 className="text-2xl font-bold">Build on a durable event store</h2>
          <p className="mt-4 text-sm text-muted-foreground">
            Start on the hosted Indie plan, or self-host the whole stack for free under Apache-2.0.
            Either way you keep full event provenance. Published microsecond-range results cover
            Core indexed reads, not end-to-end memory recall.
          </p>
          <div className="mt-6 flex flex-wrap items-center justify-center gap-4">
            <Link href="/pricing" className={cn(buttonVariants({ variant: "default" }))}>
              See pricing <ChevronRight className="ml-1 h-4 w-4" />
            </Link>
            {competitor.articleSlug ? (
              <Link
                href={`/blog/${competitor.articleSlug}`}
                className={cn(buttonVariants({ variant: "outline" }))}
              >
                Read the full {competitor.name} comparison
              </Link>
            ) : (
              <Link
                href="/event-sourcing-for-ai-agents"
                className={cn(buttonVariants({ variant: "outline" }))}
              >
                Event sourcing for AI agents
              </Link>
            )}
          </div>
        </div>
      </Section>
    </div>
  );
}
