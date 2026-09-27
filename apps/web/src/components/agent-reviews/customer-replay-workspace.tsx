"use client";
import { Button } from "@allsource/ui";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  type ReplayProposal,
  type ReplayReview,
  type ReplaySnapshot,
  replayRequest,
} from "@/lib/customer-replay-client";
import {
  type EvidenceConnection,
  expiryLabel,
  operationId,
  type SourceReference,
} from "@/lib/customer-review-client";
import { CustomerReplayReport, ReplayFacts } from "./customer-replay-report";

export function CustomerReplayWorkspace({ connection }: { connection: EvidenceConnection }) {
  const [saved, setSaved] = useState<ReplayReview[]>([]);
  const [review, setReview] = useState<ReplayReview | null>(null);
  const [target, setTarget] = useState("event-count");
  const [snapshot, setSnapshot] = useState<ReplaySnapshot | null>(null);
  const [proposal, setProposal] = useState<ReplayProposal | null>(null);
  const [consent, setConsent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const retries = useRef(new Map<string, Record<string, unknown>>());
  const active = connection.status === "active" && connection.expires_at * 1000 > Date.now();

  const refresh = useCallback(async () => {
    const result = await replayRequest<{ reviews: ReplayReview[] }>("workspace", connection.id);
    setSaved(result.reviews);
  }, [connection.id]);

  useEffect(() => {
    void refresh().catch(() => setError("Replay reviews unavailable. Refresh to retry."));
  }, [refresh]);

  useEffect(() => {
    const clear = () => {
      setReview(null);
      setSnapshot(null);
      setProposal(null);
      setConsent(false);
    };
    const hidden = () => {
      if (document.visibilityState === "hidden") clear();
    };
    const expiry = Math.min(connection.expires_at, review?.expires_at ?? Infinity);
    const timer = setTimeout(clear, Math.max(0, Math.min(3_600_000, expiry * 1000 - Date.now())));
    document.addEventListener("visibilitychange", hidden);
    return () => {
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", hidden);
    };
  }, [connection.expires_at, review?.expires_at]);

  async function act(work: () => Promise<void>) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await work();
    } catch (failure) {
      setReview(null);
      setError(failure instanceof Error ? failure.message : "Replay request unavailable.");
    } finally {
      setBusy(false);
    }
  }

  async function send<T>(
    operation: string,
    fields: Record<string, unknown>,
    key: "request_id" | "operation_id" | "idempotency_key"
  ) {
    const fingerprint = JSON.stringify([operation, fields]);
    if (!retries.current.has(fingerprint))
      retries.current.set(fingerprint, { ...fields, [key]: operationId() });
    const result = await replayRequest<T>(
      operation,
      connection.id,
      retries.current.get(fingerprint)
    );
    retries.current.delete(fingerprint);
    return result;
  }

  async function inspect() {
    setSnapshot(null);
    setProposal(null);
    setConsent(false);
    const result = await send<{ snapshot: ReplaySnapshot }>(
      "inspect",
      { projection_name: target },
      "request_id"
    );
    setSnapshot(result.snapshot);
  }

  async function share() {
    if (!snapshot || !consent) return;
    const result = await send<{ source: SourceReference }>(
      "share",
      { snapshot, ttl: 900, consent: { accepted: true, version: "selected-replay-analysis-v1" } },
      "operation_id"
    );
    setProposal({
      schema_version: 1,
      kind: "replay_plan",
      projection_name: snapshot.analysis.projection_name,
      sources: [result.source],
    });
    setNotice(
      "Analysis shared with this connection. Copy its proposal for Claude or prepare a review here."
    );
  }

  async function prepare(edit = false) {
    if (!proposal) return;
    const fields =
      edit && review
        ? { id: review.id, version: review.version, digest: review.digest, proposal }
        : { expected_revision: 0, proposal };
    const result = await send<ReplayReview>(edit ? "edit" : "prepare", fields, "idempotency_key");
    setReview(result);
    await refresh();
  }

  async function open(item: ReplayReview) {
    setReview(null);
    setReview(
      await send<ReplayReview>(
        "read",
        { id: item.id, version: item.version, digest: item.digest },
        "request_id"
      )
    );
  }

  async function decide(operation: "approve" | "reject") {
    if (!review) return;
    const identity = JSON.stringify([operation, review.id, review.version, review.digest]);
    if (!retries.current.has(identity))
      retries.current.set(identity, {
        id: review.id,
        version: review.version,
        digest: review.digest,
        decision_id: review.decision?.id ?? crypto.randomUUID(),
        request_id: operationId(),
      });
    const result = await replayRequest<ReplayReview>(
      operation,
      connection.id,
      retries.current.get(identity)
    );
    retries.current.delete(identity);
    setReview(result);
    await refresh();
  }

  return (
    <div className="min-w-0 space-y-6 border-t pt-8" aria-busy={busy}>
      <section
        className="space-y-4 rounded-xl border p-5 sm:p-6"
        aria-labelledby="replay-sources-title"
      >
        <h2 id="replay-sources-title" className="text-xl font-semibold">
          Replay plans
        </h2>
        <p>
          Inspect an enabled projection, share its analysis, then review one proposed rebuild.
          Inspection and sharing each use one query. Nothing runs until you approve an exact review
          version.
        </p>
        <label className="block space-y-2">
          <span>Enabled projection name</span>
          <input
            className="w-full rounded-md border bg-background p-3 text-base"
            value={target}
            disabled={busy || !active}
            onChange={(event) => {
              setTarget(event.target.value);
              setSnapshot(null);
              setProposal(null);
              setConsent(false);
            }}
          />
        </label>
        <Button
          className="h-auto min-h-11 whitespace-normal text-base"
          variant="outline"
          disabled={busy || !active || !target}
          onClick={() => void act(inspect)}
        >
          Inspect replay analysis · 1 query
        </Button>
        {snapshot && (
          <div className="space-y-4 border-t pt-4">
            <ReplayFacts snapshot={snapshot} />
            <label className="flex items-start gap-3">
              <input
                type="checkbox"
                className="mt-1 h-5 w-5 shrink-0"
                checked={consent}
                disabled={busy || !!proposal}
                onChange={(event) => setConsent(event.target.checked)}
              />
              <span>
                Share these analysis facts and hashes with this connection for up to 15 minutes. Raw
                event content stays private.
              </span>
            </label>
            <Button
              className="h-auto min-h-11 whitespace-normal text-base"
              disabled={busy || !active || !consent || !!proposal}
              onClick={() => void act(share)}
            >
              Share replay analysis · 1 query
            </Button>
          </div>
        )}
        {proposal && (
          <div className="space-y-4 border-t pt-4">
            <p>
              Claude can validate and prepare this proposal. Preparation uses one query and grants
              no approval.
            </p>
            <div className="flex flex-wrap gap-3">
              <Button
                variant="outline"
                className="h-auto min-h-11 whitespace-normal text-base"
                onClick={() =>
                  void act(async () => {
                    await navigator.clipboard.writeText(JSON.stringify(proposal, null, 2));
                    setNotice(
                      "Proposal copied. Ask Claude to prepare it, then refresh replay reviews."
                    );
                  })
                }
              >
                Copy replay proposal
              </Button>
              <Button
                variant="outline"
                className="h-auto min-h-11 whitespace-normal text-base"
                disabled={busy || !active}
                onClick={() => void act(() => prepare())}
              >
                Prepare review · 1 query
              </Button>
              {review?.state === "pending" && (
                <Button
                  variant="outline"
                  className="h-auto min-h-11 whitespace-normal text-base"
                  disabled={busy || !active}
                  onClick={() => void act(() => prepare(true))}
                >
                  Replace selected plan with this proposal
                </Button>
              )}
            </div>
          </div>
        )}
      </section>
      {error && (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      )}
      {notice && <p role="status">{notice}</p>}
      <section
        className="space-y-4 rounded-xl border p-5 sm:p-6"
        aria-labelledby="saved-replays-title"
      >
        <h2 id="saved-replays-title" className="text-xl font-semibold">
          Saved replay reviews
        </h2>
        <p>
          Pending reviews use one query to recheck evidence when opened. Completed decisions recover
          their recorded result. Refreshing never dispatches a replay.
        </p>
        <Button
          variant="outline"
          className="h-auto min-h-11 whitespace-normal text-base"
          disabled={busy}
          onClick={() =>
            void act(async () => {
              setReview(null);
              await refresh();
            })
          }
        >
          Refresh replay reviews
        </Button>
        {saved.length === 0 ? (
          <p>No replay reviews saved for this connection.</p>
        ) : (
          <ul className="space-y-4">
            {saved.map((item) => (
              <li className="space-y-2 border-t pt-4" key={item.id}>
                <p>
                  Version {item.version} · {item.effective_state} · Expires{" "}
                  {expiryLabel(item.expires_at)}
                </p>
                <p className="break-all font-mono">{item.id}</p>
                <Button
                  variant="outline"
                  className="h-auto min-h-11 whitespace-normal text-base"
                  disabled={busy || !active}
                  onClick={() => void act(() => open(item))}
                >
                  Open replay review {item.id.slice(0, 8)}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </section>
      {review && (
        <CustomerReplayReport
          key={`${review.id}-${review.version}-${review.effective_state}-${review.replay?.status}`}
          review={review}
          busy={busy}
          onDecision={(operation) => void act(() => decide(operation))}
        />
      )}
    </div>
  );
}
