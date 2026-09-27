import type { Metadata } from "next";
import { connection } from "next/server";
import { CustomerEvidenceWorkspace } from "@/components/agent-reviews/customer-evidence-workspace";

export const metadata: Metadata = {
  title: "Agent reviews | AllSource",
  robots: { index: false, follow: false },
};

export default async function AgentReviewsPage() {
  await connection();
  return (
    <div className="mx-auto min-w-0 max-w-5xl space-y-6 p-4 sm:p-6">
      <header className="space-y-3">
        <h1 className="text-3xl font-semibold tracking-tight">Agent reviews</h1>
        <p className="text-base leading-relaxed text-muted-foreground">
          Select recorded evidence. Compare pinned runs. Inspect what your assistant prepared.
        </p>
      </header>
      {process.env.CUSTOMER_CONNECTIONS_ENABLED === "true" &&
      process.env.CUSTOMER_EVIDENCE_ENABLED === "true" ? (
        <CustomerEvidenceWorkspace />
      ) : (
        <p className="rounded-xl border p-6 text-base">
          Agent evidence reviews are not available yet.
        </p>
      )}
    </div>
  );
}
