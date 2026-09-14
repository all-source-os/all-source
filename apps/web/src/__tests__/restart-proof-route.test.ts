import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const WEB_ROOT = path.resolve(__dirname, "../..");

function source(relativePath: string): string {
  return readFileSync(path.join(WEB_ROOT, relativePath), "utf-8");
}

describe("agent-memory restart proof route", () => {
  const page = source("src/app/(marketing)/agent-memory-restart-proof/page.tsx");
  const actions = source(
    "src/app/(marketing)/agent-memory-restart-proof/restart-proof-actions.tsx"
  );

  it("publishes one executable restart, recall, and provenance circuit", () => {
    expect(page).toContain("cargo install allsource-prime");
    expect(page).toContain("--mode http --port 3905 --data-dir");
    expect(page).toContain("/api/v1/prime/recall");
    expect(page).toContain("/history");
    expect(page).toContain("prime.node.created");
    expect(page).toContain("runbook-42");
    expect(page).toContain("Ctrl+C");
  });

  it("keeps proof claims bounded and tracks only fixed action names", () => {
    expect(page).toContain("What this proves—and what it does not");
    expect(page).not.toMatch(/guarantee|perfect memory|production-ready/i);
    expect(actions).toContain('proof_name: "agent_memory_restart_provenance"');
    expect(actions).toContain('window.gtag?.("event", "restart_proof_action"');
    expect(actions).not.toMatch(/email|api[_ -]?key|node[_ -]?id/i);
  });

  it("tags hosted handoff and keeps route discoverable", () => {
    const sitemap = source("src/app/sitemap.ts");
    const pillar = source("src/app/(marketing)/event-sourcing-for-ai-agents/page.tsx");

    expect(actions).toContain("/connect?source=restart-proof");
    expect(sitemap).toContain("/agent-memory-restart-proof");
    expect(pillar).toContain('href="/agent-memory-restart-proof"');
  });
});
