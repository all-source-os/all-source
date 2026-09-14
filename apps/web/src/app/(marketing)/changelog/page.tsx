import fs from "node:fs";
import path from "node:path";
import { Badge, Card, CardContent } from "@allsource/ui";
import { FadeIn } from "@/components/ui/fade-in";
import { siteConfig } from "@/lib/config";
import { constructMetadata } from "@/lib/utils";

export const metadata = constructMetadata({
  title: "Changelog",
  description: `Release history and updates for ${siteConfig.name}.`,
  canonical: "/changelog",
});

interface ChangelogEntry {
  version: string;
  date: string;
  sections: { heading: string; items: string[] }[];
}

function parseChangelog(content: string): ChangelogEntry[] {
  const entries: ChangelogEntry[] = [];
  let current: ChangelogEntry | null = null;
  let currentSection: { heading: string; items: string[] } | null = null;

  for (const line of content.split("\n")) {
    const versionMatch = line.match(/^## \[(.+?)\] - (.+)$/);
    if (versionMatch?.[1] && versionMatch[2]) {
      if (current) entries.push(current);
      current = {
        version: versionMatch[1],
        date: versionMatch[2],
        sections: [],
      };
      currentSection = null;
      continue;
    }

    const sectionMatch = line.match(/^### (.+)$/);
    if (sectionMatch?.[1] && current) {
      currentSection = { heading: sectionMatch[1], items: [] };
      current.sections.push(currentSection);
      continue;
    }

    if (line.startsWith("- ") && currentSection) {
      currentSection.items.push(line.slice(2));
    }
  }

  if (current) entries.push(current);
  return entries;
}

function sectionBadgeColor(heading: string): string {
  switch (heading.toLowerCase()) {
    case "added":
      return "bg-green-500/10 text-green-500 border-green-500/20";
    case "changed":
      return "bg-blue-500/10 text-blue-500 border-blue-500/20";
    case "fixed":
      return "bg-yellow-500/10 text-yellow-500 border-yellow-500/20";
    case "removed":
      return "bg-red-500/10 text-red-500 border-red-500/20";
    default:
      return "bg-muted text-muted-foreground";
  }
}

export default async function ChangelogPage() {
  const changelogPath = path.join(process.cwd(), "../../docs/CHANGELOG.md");
  const content = fs.readFileSync(changelogPath, "utf-8");
  const entries = parseChangelog(content);

  return (
    <div className="mx-auto w-full max-w-screen-md px-4 lg:px-8 py-24">
      <FadeIn delay={0.1} inView>
        <h1 className="text-3xl font-bold text-foreground sm:text-4xl mb-2">
          AllSource release notes
        </h1>
        <p className="text-lg text-muted-foreground mb-8">
          Shipped changes, fixes, and compatibility notes for {siteConfig.name}.
        </p>
      </FadeIn>

      <div className="space-y-6">
        {entries.map((entry, i) => (
          <FadeIn key={entry.version} delay={0.15 + i * 0.05} inView>
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-xl font-semibold text-foreground">v{entry.version}</h2>
                  <span className="text-sm text-muted-foreground">{entry.date}</span>
                </div>
                {entry.sections.map((section) => (
                  <div key={section.heading} className="mb-4 last:mb-0">
                    <Badge className={`mb-2 ${sectionBadgeColor(section.heading)}`}>
                      {section.heading}
                    </Badge>
                    <ul className="space-y-1 ml-1">
                      {section.items.map((item) => (
                        <li key={item} className="text-sm text-muted-foreground flex gap-2">
                          <span className="text-muted-foreground/50 shrink-0">&bull;</span>
                          <span>{item}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                ))}
              </CardContent>
            </Card>
          </FadeIn>
        ))}
      </div>
    </div>
  );
}
