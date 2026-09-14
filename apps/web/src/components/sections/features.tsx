import { Card, CardContent, Section } from "@allsource/ui";
import { Cable, FileCheck, GitBranch, History, Rows3, Workflow } from "lucide-react";
import Link from "next/link";

const capabilities = [
  {
    title: "Immutable event streams",
    content:
      "Read an entity's complete history in order, inspect each payload, and trace which event caused a state change.",
    icon: Rows3,
    href: "/event-sourcing/patterns/aggregate-streams",
  },
  {
    title: "Point-in-time reconstruction",
    content:
      "Ask what a stream contained at a sequence or timestamp and reconstruct the matching state.",
    icon: History,
    href: "/event-sourcing/patterns/temporal-queries",
  },
  {
    title: "Schema governance",
    content:
      "Register versioned event schemas and reject incompatible payloads before they become permanent history.",
    icon: FileCheck,
    href: "/event-sourcing/patterns/event-schema-evolution",
  },
  {
    title: "Stream processing",
    content:
      "Filter, map, reduce, window, or branch accepted events inline while preserving source history.",
    icon: Workflow,
    href: "/platform/stream-processing",
  },
  {
    title: "Rebuildable projections",
    content:
      "Fold existing events into tenant-scoped read models for HTTP, realtime, and analytics consumers.",
    icon: GitBranch,
    href: "/event-sourcing/patterns/projections-read-models",
  },
  {
    title: "Durable subscriptions",
    content:
      "Resume named consumers from acknowledged WAL positions, replay missed events, and continue into live delivery.",
    icon: Cable,
    href: "/event-sourcing/patterns/durable-subscriptions",
  },
];

export default function Features() {
  return (
    <Section
      title="Full event-store capabilities"
      subtitle="Core database"
      description="Immutable streams, concurrency, replay, projections, snapshots, schemas, and durable consumers share one source of truth."
    >
      <div className="grid gap-px border border-border bg-border md:grid-cols-2 lg:grid-cols-3">
        {capabilities.map((capability) => (
          <Link key={capability.title} href={capability.href} className="group bg-background">
            <Card className="h-full rounded-none border-0 bg-card shadow-none transition-colors group-hover:bg-muted/30">
              <CardContent className="flex gap-4 p-6">
                <div className="flex h-11 w-11 shrink-0 items-center justify-center border border-border bg-background transition-colors group-hover:border-primary/50">
                  <capability.icon className="h-5 w-5 text-primary" aria-hidden="true" />
                </div>
                <div>
                  <h3 className="text-lg font-semibold">{capability.title}</h3>
                  <p className="mt-2 text-sm leading-6 text-muted-foreground">
                    {capability.content}
                  </p>
                  <span className="mt-4 inline-flex items-center font-mono text-xs font-semibold text-primary">
                    Read guide →
                  </span>
                </div>
              </CardContent>
            </Card>
          </Link>
        ))}
      </div>
    </Section>
  );
}
