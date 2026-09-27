import Link from "next/link";
import { connection } from "next/server";
import { CustomerConnections } from "@/components/settings/customer-connections";
import { remoteEnabled, remoteIssuer } from "@/lib/server/customer-agent-http";

export default async function ConnectionsPage() {
  await connection();
  const issuer = remoteIssuer();
  return (
    <div className="mx-auto max-w-3xl space-y-6 p-6">
      <div>
        <h1 className="text-3xl font-semibold tracking-tight">Agent connections</h1>
        <p className="mt-3 text-base leading-relaxed text-muted-foreground">
          Choose what your assistant can read. Review decisions stay in your workspace.
        </p>
      </div>
      {remoteEnabled() && issuer && (
        <section className="space-y-4 rounded-xl border p-6 text-base leading-relaxed">
          <h2 className="text-xl font-semibold">Connect Claude on the web</h2>
          <p>In Claude, add a custom connector with this server URL:</p>
          <p className="break-all rounded-lg bg-muted p-3 font-mono">
            {issuer}/mcp/customer-review
          </p>
          <p>
            In advanced settings, use client ID <code>claude-ai</code> and leave the client secret
            empty.
          </p>
          <p>
            You will return here to sign in and review access. Connections last one hour; reconnect
            when they expire. Active and pending connections appear below for revocation.
          </p>
          <p className="text-muted-foreground">
            This connection checks eligibility and proposal syntax. Source access and review
            preparation are not available yet.
          </p>
        </section>
      )}
      {process.env.CUSTOMER_CONNECTIONS_ENABLED === "true" ? (
        <>
          {process.env.CUSTOMER_EVIDENCE_ENABLED === "true" && (
            <Link
              className="inline-block text-base underline underline-offset-4"
              href="/dashboard/tools/agent-reviews"
            >
              Open Agent reviews to share sources and inspect saved comparisons
            </Link>
          )}
          <CustomerConnections
            evidenceEnabled={process.env.CUSTOMER_EVIDENCE_ENABLED === "true"}
            replayEnabled={process.env.CUSTOMER_REPLAY_ENABLED === "true"}
          />
        </>
      ) : (
        <p className="rounded-xl border p-6 text-base">Agent connections are not available yet.</p>
      )}
    </div>
  );
}
