import { Card, CardContent, Section } from "@allsource/ui";
import { Database, GitBranch, Search } from "lucide-react";

const steps = [
  {
    number: "01",
    title: "Write events",
    content:
      "Send structured events through HTTP, WebSocket, an SDK, or MCP. AllSource appends accepted writes to an immutable stream.",
    icon: Database,
  },
  {
    number: "02",
    title: "Build useful views",
    content:
      "Query Service folds ordered Core events into tenant-scoped current-state projections and separates HTTP, realtime, and analytics reads.",
    icon: GitBranch,
  },
  {
    number: "03",
    title: "Query and replay history",
    content:
      "Serve request-response queries, push live channel updates, run analytics, or rebuild a read model from the same source history.",
    icon: Search,
  },
];

export default function HowItWorks() {
  return (
    <Section
      title="From event write to queryable state"
      subtitle="The event-store data path"
      description="One ordered history supports application state, operational analysis, and higher-level uses such as agent memory."
    >
      <ol className="grid gap-6 lg:grid-cols-3">
        {steps.map((step) => (
          <li key={step.number}>
            <Card className="h-full border-border bg-card shadow-none">
              <CardContent className="p-6">
                <div className="flex items-center justify-between">
                  <span className="font-mono text-sm font-semibold text-primary">
                    {step.number}
                  </span>
                  <step.icon className="h-5 w-5 text-muted-foreground" aria-hidden="true" />
                </div>
                <h3 className="mt-8 text-xl font-semibold">{step.title}</h3>
                <p className="mt-3 leading-7 text-muted-foreground">{step.content}</p>
              </CardContent>
            </Card>
          </li>
        ))}
      </ol>
    </Section>
  );
}
