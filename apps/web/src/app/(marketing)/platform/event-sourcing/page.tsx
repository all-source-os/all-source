"use client";

import { buttonVariants, cn, Section } from "@allsource/ui";
import {
  ChevronRight,
  Database,
  FileCheck,
  GitBranch,
  History,
  Lock,
  Shield,
  Zap,
} from "lucide-react";
import Link from "next/link";
import { staticMotion as motion } from "@/components/ui/static-motion";

const features = [
  {
    title: "Immutable Event Log",
    description:
      "Accepted state changes are append-only events with CRC32 checksums. Existing event payloads are not updated in place, so their provenance remains queryable.",
    icon: Lock,
    color: "from-blue-500/20 to-blue-500/5",
  },
  {
    title: "Time-Travel Queries",
    description:
      "Reconstruct entity state at a historical timestamp by replaying accepted events. Core's 11.9us p99 indexed-read reference does not measure this reconstructive path.",
    icon: History,
    color: "from-purple-500/20 to-purple-500/5",
  },
  {
    title: "WAL + Parquet Durability",
    description:
      "A write-ahead log supports configurable fsync before events move to Parquet files with Snappy compression for analytical reads and compact storage.",
    icon: Shield,
    color: "from-green-500/20 to-green-500/5",
  },
  {
    title: "DashMap Concurrent Reads",
    description:
      "An in-memory concurrent map serves the published 11.9us p99 read benchmark. The separate batch-ingestion benchmark reaches 469K events/sec.",
    icon: Zap,
    color: "from-yellow-500/20 to-yellow-500/5",
  },
  {
    title: "Projections & Snapshots",
    description:
      "Build materialized views that stay in sync with your event stream. Checkpoint snapshots periodically to keep recovery fast as your event volume grows.",
    icon: GitBranch,
    color: "from-cyan-500/20 to-cyan-500/5",
  },
  {
    title: "Schema Governance",
    description:
      "Register event schemas and validate payloads at ingestion so incompatible changes are rejected before they enter a stream.",
    icon: FileCheck,
    color: "from-orange-500/20 to-orange-500/5",
  },
];

const codeExample = `# Ingest an event
curl -X POST https://api.all-source.xyz/api/v1/events \\
  -H "Authorization: Bearer $API_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{
    "event_type": "order.placed",
    "entity_id": "order-123",
    "payload": { "total": 149.99, "items": 3 }
  }'

# Time-travel query: what was this order's state yesterday?
curl "https://api.all-source.xyz/api/v1/events/query?\\
  entity_id=order-123&before=2026-04-19T00:00:00Z" \\
  -H "Authorization: Bearer $API_KEY"`;

export default function EventSourcingPage() {
  return (
    <div className="pt-24">
      {/* Hero */}
      <Section className="pb-8">
        <motion.div
          className="mx-auto max-w-3xl text-center"
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6 }}
        >
          <div className="mb-4 inline-flex items-center gap-2 rounded-full border border-primary/30 bg-primary/10 px-4 py-1.5 text-sm text-primary">
            <Database className="h-4 w-4" />
            Platform
          </div>
          <h1 className="text-4xl font-bold tracking-tight sm:text-6xl">
            Purpose-built event store database for event sourcing
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-lg text-muted-foreground">
            Append state changes to ordered streams, reconstruct state at a sequence or timestamp,
            replay accepted events into new projections, and resume durable consumers from
            acknowledged positions.
          </p>
          <div className="mt-8 flex flex-col items-stretch justify-center gap-3 sm:flex-row sm:items-center">
            <Link href="/signup" className={cn(buttonVariants({ variant: "default" }))}>
              Start 14-day trial <ChevronRight className="ml-1 h-4 w-4" />
            </Link>
            <Link href="/docs/api" className={cn(buttonVariants({ variant: "outline" }))}>
              API docs
            </Link>
            <Link
              href="/event-sourcing/patterns"
              className={cn(buttonVariants({ variant: "ghost" }))}
            >
              Production patterns
            </Link>
          </div>
        </motion.div>
      </Section>

      {/* Key metrics */}
      <Section className="py-12">
        <div className="mx-auto grid max-w-4xl grid-cols-2 gap-8 sm:grid-cols-4 text-center">
          {[
            { value: "469K", label: "events/sec", sub: "batch-ingest reference" },
            { value: "11.9μs", label: "p99 latency", sub: "Core indexed-read reference" },
            { value: "WAL + Parquet", label: "durable storage", sub: "checksummed persistence" },
            { value: "55", label: "default tenant MCP tools", sub: "73 with fleet controls" },
          ].map((stat) => (
            <motion.div
              key={stat.label}
              initial={{ opacity: 0, y: 10 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
            >
              <div className="text-3xl font-bold text-primary">{stat.value}</div>
              <div className="text-sm font-medium text-foreground">{stat.label}</div>
              <div className="text-xs text-muted-foreground">{stat.sub}</div>
            </motion.div>
          ))}
        </div>
      </Section>

      {/* Features grid */}
      <Section
        title="Core event-store capabilities"
        subtitle="Database primitives for durable, queryable, and correct event history"
        className="py-16"
      >
        <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
          {features.map((feat, i) => (
            <motion.div
              key={feat.title}
              initial={{ opacity: 0, y: 20 }}
              whileInView={{ opacity: 1, y: 0 }}
              transition={{ delay: i * 0.05 }}
              viewport={{ once: true }}
              className="rounded-xl border p-6"
            >
              <div
                className={cn(
                  "mb-4 flex h-10 w-10 items-center justify-center rounded-lg bg-gradient-to-br",
                  feat.color
                )}
              >
                <feat.icon className="h-5 w-5" />
              </div>
              <h3 className="mb-2 text-lg font-semibold">{feat.title}</h3>
              <p className="text-sm text-muted-foreground">{feat.description}</p>
            </motion.div>
          ))}
        </div>
      </Section>

      {/* Code example */}
      <Section
        title="Write an event, then query its stream"
        subtitle="Ingest events and query history with plain HTTP"
        className="py-16"
      >
        <motion.div
          className="mx-auto max-w-3xl overflow-hidden rounded-xl border bg-[#0c0c14]"
          initial={{ opacity: 0, scale: 0.98 }}
          whileInView={{ opacity: 1, scale: 1 }}
          viewport={{ once: true }}
        >
          <div className="flex items-center gap-2 border-b px-4 py-3">
            <div className="h-3 w-3 rounded-full bg-red-500" />
            <div className="h-3 w-3 rounded-full bg-yellow-500" />
            <div className="h-3 w-3 rounded-full bg-green-500" />
            <span className="ml-2 text-xs text-muted-foreground">Terminal</span>
          </div>
          <pre className="overflow-x-auto p-6 text-sm leading-relaxed text-gray-300">
            <code>{codeExample}</code>
          </pre>
        </motion.div>
      </Section>

      {/* CTA */}
      <Section className="py-16 text-center">
        <h2 className="text-3xl font-bold">Write your first event</h2>
        <p className="mx-auto mt-4 max-w-lg text-muted-foreground">
          Start with a 14-day trial on the hosted plans, or self-host the open-source core for free
          under Apache-2.0.
        </p>
        <div className="mt-8 flex flex-col items-stretch justify-center gap-3 sm:flex-row sm:items-center">
          <Link
            href="/signup"
            className={cn(buttonVariants({ variant: "default", size: "lg" }), "w-full sm:w-auto")}
          >
            Start 14-day trial
          </Link>
          <Link
            href="/compare/eventstoredb"
            className={cn(buttonVariants({ variant: "outline", size: "lg" }), "w-full sm:w-auto")}
          >
            Compare with EventStoreDB
          </Link>
        </div>
      </Section>
    </div>
  );
}
