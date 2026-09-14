import { buttonVariants, Card, CardContent, cn } from "@allsource/ui";
import { ArrowRight, Database, GitBranch, Zap } from "lucide-react";
import Link from "next/link";
import { FadeIn } from "@/components/ui/fade-in";
import { siteConfig } from "@/lib/config";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "About",
  description: `Why we're building ${siteConfig.productName}: durable event history in Core, agent memory in Prime, and a managed hosted route.`,
  canonical: "/about",
});

const principles = [
  {
    title: "Events, not rows",
    icon: GitBranch,
    body: "Mutable rows lose history. AllSource records every state change as an immutable event, derives current state by projection, and lets you reconstruct any past moment with a single `as_of` query.",
  },
  {
    title: "Core IS the database",
    icon: Database,
    body: "No second database to keep in sync. Core owns event and operational-metadata durability through its WAL and Parquet storage, with DashMap-backed reads. Current services require no PostgreSQL instance.",
  },
  {
    title: "Built for agents",
    icon: Zap,
    body: "AI agents need persistent memory beyond a context window. A default tenant connector exposes 55 MCP tools for durable history; fleet operators can enable 73 with administrative controls.",
  },
];

export default function AboutPage() {
  return (
    <div className="mx-auto w-full max-w-screen-md px-4 lg:px-8 py-24">
      <FadeIn delay={0.1} inView>
        <h1 className="text-3xl font-bold text-foreground sm:text-4xl mb-2">About AllSource</h1>
        <p className="text-lg text-muted-foreground">
          Durable event history for applications. Evidence-backed memory for agents.
        </p>
      </FadeIn>

      <FadeIn delay={0.2} inView>
        <div className="prose prose-invert mt-10 max-w-none text-muted-foreground">
          <p>
            AllSource is created by Decebal Dobrica and published by Wolven Tech. Product claims on
            this site link back to public source, runnable benchmark instructions, or explicit scope
            notes so evaluators can distinguish measured evidence from roadmap work.
          </p>
          <p>
            AllSource exists because the data layer underneath modern applications — and especially
            agentic ones — often treats history as secondary. Current-state systems can preserve
            history, but teams commonly add audit tables, change-data capture, or logs after the
            primary model is designed. Time-travel debugging then spans several systems. Agents have
            to recover context on every invocation.
          </p>
          <p>
            AllSource Core is the focused answer: append-only events as source of truth, projections
            for derived state, and point-in-time inspection from one ordered history. It is written
            in Rust and persists accepted event data through a CRC32-checked WAL and Parquet files.
            Published performance numbers describe specific Core benchmark paths, not every hosted
            request or memory-recall operation.
          </p>
          <p>
            Around Core sit three named layers. Prime derives graph, vector, and temporal agent
            memory. Hosted AllSource handles tenant provisioning, authentication, quotas, billing,
            and public API access. Separate MCP connectors expose event-store or Prime operations to
            compatible agents. Chronis is a reference task-tracking application, not another
            database. Community components are Apache-2.0; designated enterprise features use BSL
            1.1.
          </p>
        </div>
      </FadeIn>

      <FadeIn delay={0.3} inView>
        <h2 className="text-2xl font-semibold text-foreground mt-16 mb-4">What we believe</h2>
      </FadeIn>

      <div className="grid gap-4 md:grid-cols-3 mt-2">
        {principles.map((p, i) => (
          <FadeIn key={p.title} delay={0.35 + i * 0.05} inView>
            <Card className="h-full">
              <CardContent className="pt-6">
                <p.icon className="h-6 w-6 text-foreground mb-3" />
                <h3 className="font-semibold text-foreground mb-2">{p.title}</h3>
                <p className="text-sm text-muted-foreground">{p.body}</p>
              </CardContent>
            </Card>
          </FadeIn>
        ))}
      </div>

      <FadeIn delay={0.5} inView>
        <div className="mt-16 flex flex-wrap gap-3">
          <Link
            href="https://github.com/all-source-os/all-source"
            className={cn(buttonVariants({ variant: "default" }))}
            target="_blank"
            rel="noopener noreferrer"
          >
            Source on GitHub <ArrowRight className="ml-2 h-4 w-4" />
          </Link>
          <Link href="/docs" className={cn(buttonVariants({ variant: "outline" }))}>
            Read the docs
          </Link>
          <Link href="/what-is-allsource" className={cn(buttonVariants({ variant: "ghost" }))}>
            Product map
          </Link>
        </div>
      </FadeIn>
    </div>
  );
}
