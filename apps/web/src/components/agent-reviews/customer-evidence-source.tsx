"use client";

import { Button } from "@allsource/ui";
import { useRef, useState } from "react";
import { customerRequest, operationId, type RunInspection } from "@/lib/customer-review-client";

export function CustomerEvidenceSource({
  connection,
  onShared,
}: {
  connection: string;
  onShared: () => Promise<void>;
}) {
  const [runId, setRunId] = useState("");
  const [inspection, setInspection] = useState<RunInspection | null>(null);
  const [accepted, setAccepted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const inspectRetry = useRef<string | null>(null);
  const shareRetry = useRef<string | null>(null);
  const valid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(runId);

  async function inspectRun() {
    setBusy(true);
    setError("");
    setNotice("");
    setInspection(null);
    setAccepted(false);
    shareRetry.current = null;
    try {
      inspectRetry.current ??= operationId();
      const result = await customerRequest<RunInspection>("inspect-run", {
        connection_id: connection,
        source: { run_id: runId, request_id: inspectRetry.current },
      });
      setInspection(result);
      inspectRetry.current = null;
    } catch (failure) {
      setError(
        failure instanceof Error
          ? failure.message
          : "Inspection unavailable. Retry the same request."
      );
    } finally {
      setBusy(false);
    }
  }

  async function share() {
    if (!inspection || !accepted) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      shareRetry.current ??= operationId();
      await customerRequest("share", {
        connection_id: connection,
        source: {
          run_id: inspection.run_id,
          revision: inspection.revision,
          sha256: inspection.sha256,
          operation_id: shareRetry.current,
          ttl: 3600,
          consent: { accepted: true, version: "selected-run-evidence-v1" },
        },
      });
      setInspection(null);
      setAccepted(false);
      shareRetry.current = null;
      setNotice("Run shared. Its pinned reference is saved below.");
      await onShared();
    } catch (failure) {
      setError(
        failure instanceof Error
          ? failure.message
          : "Sharing may have completed. Retry the same request or refresh saved work."
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="space-y-4 rounded-xl border p-5 sm:p-6" aria-labelledby="share-run-title">
      <h2 id="share-run-title" className="text-xl font-semibold">
        1. Select run evidence
      </h2>
      <p>
        Enter a run ID from your recorded run receipt. Inspecting uses 1 query and does not share it
        with Claude.
      </p>
      <label className="block space-y-2">
        <span>Recorded run ID</span>
        <input
          className="w-full rounded-md border bg-background p-3 font-mono text-base"
          autoComplete="off"
          value={runId}
          disabled={busy}
          placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
          onChange={(event) => {
            setRunId(event.target.value.trim());
            setInspection(null);
            setAccepted(false);
            setError("");
            setNotice("");
            inspectRetry.current = null;
            shareRetry.current = null;
          }}
        />
      </label>
      <Button
        className="h-auto min-h-11 whitespace-normal text-base"
        variant="outline"
        disabled={!valid || busy}
        onClick={() => void inspectRun()}
      >
        {inspectRetry.current ? "Retry same inspection · 1 query" : "Inspect run · 1 query"}
      </Button>
      {inspection && (
        <div className="space-y-4 border-t pt-4">
          <p>
            Revision {inspection.revision} · {inspection.changes}{" "}
            {inspection.changes === 1 ? "change" : "changes"} · {inspection.attempts}{" "}
            {inspection.attempts === 1 ? "attempt" : "attempts"} ·{" "}
            {inspection.completed ? "Completed" : "Incomplete"}
          </p>
          <p className="break-all font-mono">SHA-256: {inspection.sha256}</p>
          {inspection.unknowns.length > 0 && <p>Unknowns: {inspection.unknowns.join(", ")}</p>}
          <p>
            Share this exact revision with this Claude connection for up to one hour, capped by
            connection and plan expiry. Selected run metadata, test outcomes and evidence hashes may
            be processed by Anthropic under your Claude settings. Sharing uses 1 query.
          </p>
          <label className="flex items-start gap-3">
            <input
              type="checkbox"
              className="mt-1 h-5 w-5 shrink-0"
              checked={accepted}
              disabled={busy}
              onChange={(event) => setAccepted(event.target.checked)}
            />
            <span>I allow Claude to use this pinned run as comparison evidence.</span>
          </label>
          <Button
            className="h-auto min-h-11 whitespace-normal text-base"
            disabled={busy || !accepted}
            onClick={() => void share()}
          >
            {shareRetry.current ? "Retry same share · 1 query" : "Share this revision · 1 query"}
          </Button>
        </div>
      )}
      <p className="text-muted-foreground">
        Interrupted requests keep their retry key while this page stays open. Exact retries do not
        add another charge. After reload, check saved work before sharing again.
      </p>
      {error && (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
    </section>
  );
}
