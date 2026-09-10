import { Card, CardContent, Section } from "@allsource/ui";
import { GitBranch, History, Search } from "lucide-react";

const problems = [
  {
    title: "Current state hides its history",
    description:
      "A row shows what is true now, not which accepted facts produced it. Debugging and audit work then depend on scattered logs and application-specific fixes.",
    icon: History,
  },
  {
    title: "Every read model becomes permanent",
    description:
      "When current-state tables become source of truth, changing a projection means risky migrations. Durable events let teams rebuild new views beside old ones.",
    icon: GitBranch,
  },
  {
    title: "Historical queries become projects",
    description:
      "Reconstructing state at a past timestamp usually means joining logs, snapshots, and database records. An ordered event history makes that query explicit.",
    icon: Search,
  },
];

export default function Problem() {
  return (
    <Section
      title="What breaks without an event store"
      subtitle="Why event-sourced systems need durable history"
    >
      <div className="mt-12 grid grid-cols-1 gap-6 md:grid-cols-3">
        {problems.map((problem) => (
          <Card key={problem.title} className="h-full border-border bg-card shadow-none">
            <CardContent className="space-y-4 p-6">
              <div className="flex h-11 w-11 items-center justify-center border border-border bg-background">
                <problem.icon className="h-5 w-5 text-primary" aria-hidden="true" />
              </div>
              <h3 className="text-xl font-semibold">{problem.title}</h3>
              <p className="leading-7 text-muted-foreground">{problem.description}</p>
            </CardContent>
          </Card>
        ))}
      </div>
    </Section>
  );
}
