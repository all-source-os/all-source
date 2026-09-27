import type { Metadata } from "next";
import { cookies } from "next/headers";
import Link from "next/link";
import { connection } from "next/server";
import {
  claudeCallback,
  productSession,
  queryConnection,
  remoteEnabled,
  remoteIssuer,
  remoteRequestCookie,
} from "@/lib/server/customer-agent-http";

export const metadata: Metadata = {
  title: "Connect Claude | AllSource",
  robots: { index: false, follow: false },
  referrer: "no-referrer",
};

export default async function ClaudeConsentPage({
  searchParams,
}: {
  searchParams: Promise<{ error?: string }>;
}) {
  await connection();
  const cookieStore = await cookies();
  const requestToken = cookieStore.get(remoteRequestCookie)?.value;
  const session = cookieStore.get("auth_token")?.value;
  const claims = productSession(session);
  const { error } = await searchParams;
  let request: { resource: string } | null = null;
  let member = false;
  if (remoteEnabled() && requestToken && requestToken.length <= 3800) {
    try {
      const inspected = await queryConnection(
        "oauth/inspect",
        JSON.stringify({ request_token: requestToken })
      );
      if (
        remoteIssuer() &&
        inspected.ok &&
        inspected.data.issuer === remoteIssuer() &&
        inspected.data.redirect_uri === claudeCallback &&
        typeof inspected.data.resource === "string"
      )
        request = { resource: inspected.data.resource };
      if (request && claims) member = (await queryConnection("connections/list", "{}", session)).ok;
    } catch {
      request = null;
    }
  }
  return (
    <main
      className="ph-no-capture ph-no-autocapture mx-auto max-w-2xl space-y-6 px-6 py-16 text-base leading-relaxed"
      data-private="true"
    >
      <Link href="/" className="font-semibold">
        AllSource
      </Link>
      <h1 className="text-3xl font-semibold tracking-tight">Connect Claude to your workspace</h1>
      {!request ? (
        <p role="status">
          This connection request is unavailable or has expired. Start a new connection from Claude.
        </p>
      ) : (
        <>
          {error && (
            <p role="alert" className="rounded-xl border p-4">
              {error === "denied"
                ? "A verified workspace session, an eligible plan and your consent are required."
                : "The connection could not be completed. Retry, or start a new connection from Claude."}
            </p>
          )}
          <section className="space-y-4 rounded-xl border p-6">
            <h2 className="text-xl font-semibold">What Claude can access</h2>
            <p>
              For one hour, Claude can read your workspace identity, membership role and MCP
              entitlement, and validate review proposals you provide.
            </p>
            <p>
              Event content stays private. This connection cannot approve reviews or execute
              changes. Data returned to Claude may be processed by Anthropic under your Claude
              account settings.
            </p>
            <p>
              You will return to <strong>claude.ai</strong> after your decision.
            </p>
            <p className="break-all text-muted-foreground">Connection: {request.resource}</p>
          </section>
          {member && claims ? (
            <form action="/api/customer-agent/oauth/decision" method="post" className="space-y-5">
              <p>
                Workspace: <strong>{String(claims.tenant_id)}</strong>
              </p>
              <label className="flex items-start gap-3">
                <input
                  type="checkbox"
                  name="consent"
                  value="yes"
                  required
                  className="mt-1 h-5 w-5 shrink-0"
                />
                <span>I allow Claude to access these fields in this workspace.</span>
              </label>
              <div className="flex flex-wrap gap-4">
                <button
                  type="submit"
                  name="decision"
                  value="allow"
                  className="rounded-lg bg-primary px-5 py-3 font-medium text-primary-foreground"
                >
                  Allow connection
                </button>
                <button
                  type="submit"
                  name="decision"
                  value="deny"
                  formNoValidate
                  className="rounded-lg border px-5 py-3 font-medium"
                >
                  Cancel
                </button>
              </div>
            </form>
          ) : (
            <div className="space-y-4">
              <p>Sign in with a verified AllSource workspace account to review this request.</p>
              <form action="/api/customer-agent/oauth/decision" method="post">
                <button
                  type="submit"
                  name="decision"
                  value="signin"
                  className="inline-block rounded-lg bg-primary px-5 py-3 font-medium text-primary-foreground"
                >
                  Sign in to continue
                </button>
              </form>
              <form action="/api/customer-agent/oauth/decision" method="post">
                <button
                  type="submit"
                  name="decision"
                  value="deny"
                  className="rounded-lg border px-5 py-3 font-medium"
                >
                  Cancel
                </button>
              </form>
            </div>
          )}
          <p className="text-muted-foreground">
            Revoke access at any time in{" "}
            <Link href="/dashboard/settings/connections" className="underline">
              Agent connections
            </Link>
            . Your review decisions stay in AllSource.
          </p>
        </>
      )}
    </main>
  );
}
