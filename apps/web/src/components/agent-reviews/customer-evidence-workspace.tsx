"use client";

import { Button } from "@allsource/ui";
import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  customerRequest,
  type EvidenceConnection,
  type EvidenceReview,
  type EvidenceWorkspace,
  expiryLabel,
  operationId,
  type SavedReview,
} from "@/lib/customer-review-client";
import { CustomerEvidenceReport } from "./customer-evidence-report";
import { CustomerEvidenceSource } from "./customer-evidence-source";

export function CustomerEvidenceWorkspace() {
  const [connections, setConnections] = useState<EvidenceConnection[]>([]);
  const [selected, setSelected] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    setSelected("");
    setConnections([]);
    try {
      const result = await customerRequest<{ connections: EvidenceConnection[] }>("list", {});
      setConnections(
        result.connections.filter((item) => item.consent?.version === "review-evidence-v2")
      );
    } catch {
      setError("Connections could not be loaded. Retry below.");
    } finally {
      setLoading(false);
    }
  }, []);
  useEffect(() => {
    void reload();
  }, [reload]);
  const connection = connections.find((item) => item.id === selected);

  return (
    <div
      className="ph-no-capture ph-no-autocapture min-w-0 space-y-6 text-base leading-relaxed"
      data-private="true"
    >
      <section className="space-y-4 rounded-xl border p-5 sm:p-6">
        <h2 className="text-xl font-semibold">Choose a connection</h2>
        <p>
          Sources and reviews belong to one connection. Saved work recovers from your workspace
          after reload. Opening this page does not run comparisons.
        </p>
        <Link
          href="/dashboard/settings/connections"
          className="inline-block underline underline-offset-4"
        >
          Manage connections and disclosure consent
        </Link>
        <label className="block space-y-2">
          <span>Evidence connection</span>
          <select
            value={selected}
            disabled={loading}
            onChange={(event) => setSelected(event.target.value)}
            className="w-full rounded-md border bg-background p-3 text-base"
          >
            <option value="">Select a connection</option>
            {connections.map((item) => (
              <option key={item.id} value={item.id}>
                {item.client_id} · {item.id.slice(0, 8)} · {item.status}
              </option>
            ))}
          </select>
        </label>
        {loading ? (
          <p role="status">Loading connections…</p>
        ) : (
          !error &&
          connections.length === 0 && (
            <p>
              No evidence connections. Create one with “Metadata and selected run evidence” in Agent
              connections.
            </p>
          )
        )}
        <Button
          className="h-auto min-h-11 whitespace-normal text-base"
          variant="outline"
          disabled={loading}
          onClick={() => void reload()}
        >
          Refresh connections
        </Button>
        {error && (
          <p role="alert" className="text-destructive">
            {error}
          </p>
        )}
      </section>
      {connection && <ConnectionWork key={connection.id} connection={connection} />}
    </div>
  );
}

function ConnectionWork({ connection }: { connection: EvidenceConnection }) {
  const [saved, setSaved] = useState<EvidenceWorkspace | null>(null);
  const [review, setReview] = useState<EvidenceReview | null>(null);
  const [baseline, setBaseline] = useState("");
  const [candidate, setCandidate] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const readRetry = useRef<{ id: string; request: string } | null>(null);

  const refresh = useCallback(async () => {
    setReview(null);
    setSaved(null);
    setBaseline("");
    setCandidate("");
    const result = await customerRequest<EvidenceWorkspace>("workspace", {
      connection_id: connection.id,
    });
    setSaved(result);
  }, [connection.id]);

  useEffect(() => {
    let active = true;
    setBusy(true);
    void refresh()
      .catch(() => {
        if (active) setError("Saved work unavailable. Refresh to retry.");
      })
      .finally(() => {
        if (active) setBusy(false);
      });
    return () => {
      active = false;
    };
  }, [refresh]);

  useEffect(() => {
    if (!review) return;
    const expire = () => {
      setReview(null);
      setNotice("Review view cleared. Reopen to check current evidence and access.");
    };
    const hidden = () => {
      if (document.visibilityState === "hidden") expire();
    };
    const timer = setTimeout(
      expire,
      Math.min(3_600_000, Math.max(0, review.expires_at * 1000 - Date.now()))
    );
    document.addEventListener("visibilitychange", hidden);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", hidden);
    };
  }, [review]);

  async function act(work: () => Promise<void>) {
    setBusy(true);
    setError("");
    setNotice("");
    setReview(null);
    try {
      await work();
    } catch (failure) {
      setError(
        failure instanceof Error
          ? failure.message
          : "Request unavailable. Retry or refresh saved work."
      );
    } finally {
      setBusy(false);
    }
  }

  async function open(receipt: SavedReview) {
    if (readRetry.current?.id !== receipt.id)
      readRetry.current = { id: receipt.id, request: operationId() };
    const result = await customerRequest<EvidenceReview>("read-review", {
      connection_id: connection.id,
      id: receipt.id,
      version: receipt.version,
      request_id: readRetry.current.request,
    });
    setReview(result);
    readRetry.current = null;
  }

  async function copyProposal() {
    const first = saved?.sources.find(
      (item) => item.source.ref === baseline && item.status === "saved"
    );
    const second = saved?.sources.find(
      (item) => item.source.ref === candidate && item.status === "saved"
    );
    if (!first || !second || first === second) return;
    try {
      await navigator.clipboard.writeText(
        JSON.stringify(
          {
            schema_version: 1,
            kind: "run_comparison",
            projection_name: null,
            sources: [first.source, second.source],
          },
          null,
          2
        )
      );
      setNotice(
        "Proposal copied. Ask Claude to validate and prepare this comparison, then refresh saved work here. No credential was copied."
      );
    } catch {
      setError("Clipboard unavailable. Allow clipboard access and retry.");
    }
  }

  const active = saved?.connection_status === "active" && connection.expires_at * 1000 > Date.now();
  const available = saved?.sources.filter((item) => item.status === "saved") ?? [];

  return (
    <div className="min-w-0 space-y-6" aria-busy={busy}>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p>Connection expires {expiryLabel(connection.expires_at)}</p>
        <Button
          className="h-auto min-h-11 whitespace-normal text-base"
          variant="outline"
          disabled={busy}
          onClick={() => void act(refresh)}
        >
          Refresh saved work
        </Button>
      </div>
      {error && (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      {active ? (
        <CustomerEvidenceSource connection={connection.id} onShared={refresh} />
      ) : (
        <p>
          New evidence requires an active connection. Saved source references remain available for
          revocation.
        </p>
      )}
      <section
        className="space-y-4 rounded-xl border p-5 sm:p-6"
        aria-labelledby="saved-sources-title"
      >
        <h2 id="saved-sources-title" className="text-xl font-semibold">
          2. Shared sources
        </h2>
        <p>
          Saved status is a record, not a freshness check. Preparing or opening a comparison
          rechecks every source. Revoking stops future access; it cannot remove data already
          returned to Claude.
        </p>
        {!saved ? (
          <p>Saved sources unavailable.</p>
        ) : saved.sources.length === 0 ? (
          <p>No sources shared with this connection.</p>
        ) : (
          <ul className="space-y-4">
            {saved.sources.map((item) => (
              <li key={item.source.ref} className="min-w-0 space-y-2 border-t pt-4">
                <p className="break-all font-mono">Run {item.run_id}</p>
                <p>
                  Revision {item.source.revision} · {item.status} · Expires{" "}
                  {expiryLabel(item.expires_at)}
                </p>
                <details>
                  <summary className="cursor-pointer">Pinned reference and hash</summary>
                  <p className="mt-2 break-all font-mono">{item.source.ref}</p>
                  <p className="break-all font-mono">{item.source.sha256}</p>
                </details>
                {item.status !== "revoked" && (
                  <Button
                    className="h-auto min-h-11 whitespace-normal text-base"
                    variant="outline"
                    disabled={busy}
                    onClick={() =>
                      void act(async () => {
                        await customerRequest("revoke-source", {
                          connection_id: connection.id,
                          source_id: item.source.ref,
                        });
                        await refresh();
                        setNotice("Source revoked for this connection.");
                      })
                    }
                  >
                    Revoke source {item.source.ref.slice(0, 8)}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        )}
        {active && available.length >= 2 && (
          <div className="space-y-4 border-t pt-4">
            <p>
              Choose the comparison order, then ask Claude to prepare it. Preparation uses 2
              queries.
            </p>
            <div className="grid gap-4 sm:grid-cols-2">
              {[
                { label: "Baseline source", value: baseline, set: setBaseline },
                { label: "Candidate source", value: candidate, set: setCandidate },
              ].map((field) => (
                <label key={field.label} className="block min-w-0 space-y-2">
                  <span>{field.label}</span>
                  <select
                    className="w-full rounded-md border bg-background p-3 text-base"
                    value={field.value}
                    disabled={busy}
                    onChange={(event) => field.set(event.target.value)}
                  >
                    <option value="">Select a source</option>
                    {available.map((item) => (
                      <option key={item.source.ref} value={item.source.ref}>
                        {item.run_id} · r{item.source.revision}
                      </option>
                    ))}
                  </select>
                </label>
              ))}
            </div>
            <Button
              className="h-auto min-h-11 whitespace-normal text-base"
              variant="outline"
              disabled={busy || !baseline || !candidate || baseline === candidate}
              onClick={() => void copyProposal()}
            >
              Copy comparison proposal
            </Button>
          </div>
        )}
      </section>
      <section
        className="space-y-4 rounded-xl border p-5 sm:p-6"
        aria-labelledby="saved-reviews-title"
      >
        <h2 id="saved-reviews-title" className="text-xl font-semibold">
          3. Saved comparisons
        </h2>
        <p>
          Refresh after Claude prepares a comparison. Opening a review uses 2 queries. Exact retries
          reuse their original charge while this page stays open. Nothing is approved or executed
          here.
        </p>
        {!saved ? (
          <p>Saved reviews unavailable.</p>
        ) : saved.reviews.length === 0 ? (
          <p>No comparisons saved for this connection.</p>
        ) : (
          <ul className="space-y-4">
            {saved.reviews.map((item) => (
              <li key={item.id} className="space-y-3 border-t pt-4">
                <p className="break-all font-mono">{item.id}</p>
                <p>
                  Version {item.version} · {item.status} · Expires {expiryLabel(item.expires_at)}
                </p>
                <Button
                  className="h-auto min-h-11 whitespace-normal text-base"
                  variant="outline"
                  disabled={busy || !active || item.status !== "saved"}
                  onClick={() => void act(() => open(item))}
                >
                  {readRetry.current?.id === item.id
                    ? "Retry same review · 2 queries"
                    : `Open review ${item.id.slice(0, 8)} · 2 queries`}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </section>
      {review && <CustomerEvidenceReport review={review} />}
    </div>
  );
}
