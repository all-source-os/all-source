"use client";

import { Button } from "@allsource/ui";
import { useCallback, useEffect, useState } from "react";
import { CustomerConnectionSetup, type IssuedConnection } from "./customer-connection-setup";

type Connection = {
  id: string;
  client_id: string;
  resource: string;
  expires_at: number;
  operations: string[];
  status: "active" | "expired" | "revoked";
};

async function request<T>(operation: string, body: unknown): Promise<T> {
  const response = await fetch(`/api/customer-agent/connections/${operation}`, {
    method: "POST",
    credentials: "same-origin",
    cache: "no-store",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(50_000),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error?.message || "Connection request failed.");
  return result.data as T;
}

export function CustomerConnections() {
  const [connections, setConnections] = useState<Connection[]>([]);
  const [accepted, setAccepted] = useState(false);
  const [issued, setIssued] = useState<IssuedConnection | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const refresh = useCallback(async () => {
    const data = await request<{ connections: Connection[] }>("list", {});
    setConnections(data.connections);
  }, []);
  useEffect(() => {
    void refresh()
      .catch(() => setError("Connections could not be loaded. Retry below."))
      .finally(() => setLoading(false));
  }, [refresh]);

  async function act(operation: "create" | "revoke", id?: string) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      if (operation === "create") {
        const result = await request<IssuedConnection>("create", {
          client_id: "claude-code",
          operations: ["read_context", "validate_proposal"],
          ttl: 3600,
          consent: { accepted, version: "review-metadata-v1" },
        });
        setIssued(result);
        setAccepted(false);
      } else {
        await request("revoke", { id });
        if (issued?.id === id) setIssued(null);
        setNotice("Connection revoked. Your assistant can no longer use it.");
      }
      await refresh();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "Connection request failed.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="ph-no-capture ph-no-autocapture space-y-6 text-base leading-relaxed"
      data-private="true"
    >
      <section className="space-y-4 rounded-xl border p-6" aria-labelledby="connection-consent">
        <h2 id="connection-consent" className="text-xl font-semibold">
          Connect Claude Code
        </h2>
        <p>
          For one hour, Claude Code can read your workspace identity, membership role and MCP
          entitlement, and validate review proposals you provide.
        </p>
        <p className="text-muted-foreground">
          Event content stays private. This connection cannot approve reviews or execute changes.
          Data returned to Claude may be processed by Anthropic under your Claude account settings.
        </p>
        <label className="flex items-start gap-3">
          <input
            type="checkbox"
            className="mt-1 h-5 w-5 shrink-0"
            checked={accepted}
            onChange={(event) => setAccepted(event.target.checked)}
          />
          <span>I allow Claude Code to access these fields for this workspace.</span>
        </label>
        <Button
          disabled={!accepted || busy || loading || issued !== null}
          onClick={() => void act("create")}
        >
          Create connection
        </Button>
      </section>
      {issued && (
        <section className="space-y-4 rounded-xl border p-6" aria-labelledby="one-time-secret">
          <h2 id="one-time-secret" className="text-xl font-semibold">
            Install your connection
          </h2>
          <p>
            Shown once. Expires {new Date(issued.expires_at * 1000).toLocaleString()}. Keep it
            private; never paste it into a chat.
          </p>
          <label className="block" htmlFor="connection-credential">
            Connection credential
          </label>
          <input
            id="connection-credential"
            type="password"
            readOnly
            value={issued.token}
            autoComplete="off"
            className="w-full rounded-md border bg-background p-3 font-mono text-base"
          />
          <CustomerConnectionSetup key={issued.id} issued={issued} />
          <div className="flex flex-wrap gap-3">
            <Button variant="outline" onClick={() => setIssued(null)}>
              Hide credential
            </Button>
            <Button variant="outline" disabled={busy} onClick={() => void act("revoke", issued.id)}>
              Revoke connection
            </Button>
          </div>
        </section>
      )}
      {error && (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <section className="space-y-4" aria-labelledby="your-connections">
        <div className="flex items-center justify-between gap-4">
          <h2 id="your-connections" className="text-xl font-semibold">
            Your connections
          </h2>
          <Button
            variant="outline"
            disabled={busy}
            onClick={() => {
              setError("");
              void refresh().catch(() => setError("Connections could not be loaded."));
            }}
          >
            Refresh
          </Button>
        </div>
        {loading ? (
          <p>Loading connections…</p>
        ) : connections.length === 0 ? (
          <p>No connections to display.</p>
        ) : (
          <ul className="space-y-3">
            {connections.map((connection) => (
              <li key={connection.id} className="space-y-3 rounded-xl border p-5">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <h3 className="font-semibold">
                    {connection.client_id === "claude-code" ? "Claude Code" : "Claude"}
                  </h3>
                  <span className="capitalize">{connection.status}</span>
                </div>
                <p>Expires {new Date(connection.expires_at * 1000).toLocaleString()}</p>
                <p className="break-all text-muted-foreground">{connection.resource}</p>
                <p className="font-mono text-base">{connection.id}</p>
                {connection.status === "active" && (
                  <Button
                    variant="outline"
                    disabled={busy}
                    onClick={() => void act("revoke", connection.id)}
                  >
                    Revoke {connection.id.slice(0, 8)}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
