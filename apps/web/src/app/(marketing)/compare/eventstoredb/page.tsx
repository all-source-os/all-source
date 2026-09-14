"use client";

import { buttonVariants, cn, Section } from "@allsource/ui";
import { Check, ChevronRight, Minus, X } from "lucide-react";
import Link from "next/link";
import { staticMotion as motion } from "@/components/ui/static-motion";

type Cell = "yes" | "no" | "partial" | string;

interface Row {
  feature: string;
  allsource: Cell;
  eventstoredb: Cell;
  note?: string;
}

const rows: Row[] = [
  {
    feature: "Append-only event log",
    allsource: "yes",
    eventstoredb: "yes",
    note: "Both are durable event stores at their core.",
  },
  {
    feature: "Ingestion throughput",
    allsource: "469K events/sec (published batch reference)",
    eventstoredb: "~15K events/sec",
    note: "AllSource figure is its published batch-ingest reference, not a synchronous per-event durability comparison. EventStoreDB figure varies by workload and deployment.",
  },
  {
    feature: "p99 query latency",
    allsource: "11.9μs p99 (published indexed-read reference)",
    eventstoredb: "1–10ms",
    note: "AllSource figure covers Core indexed reads, not end-to-end API, replay, graph, or vector queries. Benchmark paths are not directly equivalent.",
  },
  {
    feature: "Docker image size",
    allsource: "~15.7MB",
    eventstoredb: "~280MB",
    note: "Rust static binary vs .NET runtime.",
  },
  {
    feature: "HTTP-native API",
    allsource: "yes",
    eventstoredb: "partial",
    note: "AllSource is HTTP-first. EventStoreDB's primary API is gRPC; HTTP is secondary and limited.",
  },
  {
    feature: "Temporal / as_of queries",
    allsource: "yes",
    eventstoredb: "partial",
    note: "AllSource supports point-in-time projections as a first-class feature.",
  },
  {
    feature: "Embedded mode",
    allsource: "yes",
    eventstoredb: "no",
    note: "AllSource ships as a library you can embed in-process. EventStoreDB is always a separate server.",
  },
  {
    feature: "MCP tools for AI agents",
    allsource: "55 default / 73 fleet",
    eventstoredb: "no",
    note: "AllSource ships a Model Context Protocol server out of the box. EventStoreDB has no equivalent.",
  },
  {
    feature: "Vector / semantic search",
    allsource: "yes",
    eventstoredb: "no",
    note: "AllSource Prime includes HNSW vector indexing and hybrid recall.",
  },
  {
    feature: "Knowledge graph layer",
    allsource: "yes",
    eventstoredb: "no",
    note: "AllSource Prime stores graph nodes and edges as events.",
  },
  {
    feature: "License",
    allsource: "Open source",
    eventstoredb: "ESL (source-available)",
    note: "EventStoreDB moved from Apache 2.0 to the Event Store License in 2023.",
  },
  {
    feature: "Managed cloud offering",
    allsource: "yes",
    eventstoredb: "yes",
  },
];

function CellIcon({ value }: { value: Cell }) {
  if (value === "yes") return <Check className="h-4 w-4 text-green-500" />;
  if (value === "no") return <X className="h-4 w-4 text-red-500/70" />;
  if (value === "partial") return <Minus className="h-4 w-4 text-yellow-500" />;
  return <span className="text-sm font-medium">{value}</span>;
}

export default function CompareEventStoreDBPage() {
  return (
    <div className="relative overflow-hidden">
      {/* Hero */}
      <Section className="relative pt-24 pb-16 text-center">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6 }}
        >
          <h1 className="text-4xl font-bold tracking-tight sm:text-5xl">
            AllSource vs EventStoreDB
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-lg text-muted-foreground">
            AllSource and EventStoreDB are both event stores. AllSource's published reference
            benchmark measured 11.9µs p99 reads; its default tenant MCP exposes 55 tools, rising to
            73 with fleet controls. Hosted AllSource starts at £18.99/month. This page compares
            architecture, published performance, AI integration, and licensing.
          </p>
          <div className="mt-8 flex items-center justify-center gap-4">
            <Link href="/signup" className={cn(buttonVariants({ variant: "default" }))}>
              Try AllSource <ChevronRight className="ml-1 h-4 w-4" />
            </Link>
            <Link href="/docs" className={cn(buttonVariants({ variant: "outline" }))}>
              Read the docs
            </Link>
          </div>
        </motion.div>
      </Section>

      {/* Comparison table */}
      <Section className="pb-16">
        <div className="mx-auto max-w-5xl overflow-hidden rounded-xl border">
          <div className="overflow-x-auto">
            <table className="w-full">
              <thead className="bg-muted/40 text-left">
                <tr>
                  <th className="px-6 py-4 text-sm font-semibold">Feature</th>
                  <th className="px-6 py-4 text-sm font-semibold">AllSource</th>
                  <th className="px-6 py-4 text-sm font-semibold">EventStoreDB</th>
                </tr>
              </thead>
              <tbody className="divide-y">
                {rows.map((row) => (
                  <tr key={row.feature} className="hover:bg-muted/20">
                    <td className="px-6 py-4 align-top">
                      <div className="text-sm font-medium">{row.feature}</div>
                      {row.note && (
                        <div className="mt-1 text-xs text-muted-foreground">{row.note}</div>
                      )}
                    </td>
                    <td className="px-6 py-4 align-top">
                      <CellIcon value={row.allsource} />
                    </td>
                    <td className="px-6 py-4 align-top">
                      <CellIcon value={row.eventstoredb} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </Section>

      <Section className="pb-12">
        <div className="mx-auto max-w-3xl border-y border-border py-6">
          <h2 className="font-mono text-xs uppercase tracking-[0.18em] text-muted-foreground">
            Primary sources checked
          </h2>
          <ul className="mt-4 flex flex-wrap gap-x-6 gap-y-3 text-sm">
            <li>
              <Link
                href="https://docs.kurrent.io/"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex min-h-12 items-center text-primary underline underline-offset-4"
              >
                EventStoreDB / Kurrent documentation ↗
              </Link>
            </li>
            <li>
              <Link
                href="https://github.com/EventStore/EventStore/blob/master/LICENSE.md"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex min-h-12 items-center text-primary underline underline-offset-4"
              >
                EventStoreDB licence ↗
              </Link>
            </li>
            <li>
              <Link
                href="/blog/reproduce-the-469k-events-benchmark"
                className="inline-flex min-h-12 items-center text-primary underline underline-offset-4"
              >
                Reproduce AllSource benchmark
              </Link>
            </li>
          </ul>
        </div>
      </Section>

      {/* When to pick which */}
      <Section className="pb-16">
        <div className="mx-auto grid max-w-5xl gap-6 md:grid-cols-2">
          <div className="rounded-xl border p-6">
            <h3 className="mb-3 text-lg font-semibold">Pick AllSource if…</h3>
            <ul className="space-y-2 text-sm">
              <li className="flex items-start gap-2">
                <Check className="mt-0.5 h-4 w-4 shrink-0 text-green-500" />
                You need microsecond-latency projection reads
              </li>
              <li className="flex items-start gap-2">
                <Check className="mt-0.5 h-4 w-4 shrink-0 text-green-500" />
                You're building AI agents that need persistent memory
              </li>
              <li className="flex items-start gap-2">
                <Check className="mt-0.5 h-4 w-4 shrink-0 text-green-500" />
                You want an embedded mode, not a separate server
              </li>
              <li className="flex items-start gap-2">
                <Check className="mt-0.5 h-4 w-4 shrink-0 text-green-500" />
                HTTP-native APIs matter to your stack
              </li>
              <li className="flex items-start gap-2">
                <Check className="mt-0.5 h-4 w-4 shrink-0 text-green-500" />
                You want a small production footprint (~129MB total)
              </li>
            </ul>
          </div>
          <div className="rounded-xl border p-6">
            <h3 className="mb-3 text-lg font-semibold">Pick EventStoreDB if…</h3>
            <ul className="space-y-2 text-sm">
              <li className="flex items-start gap-2">
                <Check className="mt-0.5 h-4 w-4 shrink-0 text-green-500" />
                You're deep in the .NET ecosystem and want mature tooling
              </li>
              <li className="flex items-start gap-2">
                <Check className="mt-0.5 h-4 w-4 shrink-0 text-green-500" />
                Your team already runs it in production at scale
              </li>
              <li className="flex items-start gap-2">
                <Check className="mt-0.5 h-4 w-4 shrink-0 text-green-500" />
                You rely on its subscription/persistent-subscription model
              </li>
            </ul>
          </div>
        </div>
      </Section>

      {/* CTA */}
      <Section className="pb-24">
        <div className="mx-auto max-w-2xl rounded-xl border bg-muted/20 p-8 text-center">
          <h2 className="text-2xl font-bold">Want a deeper comparison?</h2>
          <p className="mt-4 text-sm text-muted-foreground">
            Run AllSource locally in under a minute and try the same workload you'd run on
            EventStoreDB.
          </p>
          <div className="mt-6">
            <Link href="/docs/tenant-setup" className={cn(buttonVariants({ variant: "default" }))}>
              Quickstart <ChevronRight className="ml-1 h-4 w-4" />
            </Link>
          </div>
        </div>
      </Section>
    </div>
  );
}
