"use client";
import { Button } from "@allsource/ui";
import { useEffect, useRef } from "react";
import type { ReplayReview, ReplaySnapshot } from "@/lib/customer-replay-client";
import { expiryLabel } from "@/lib/customer-review-client";

export function ReplayFacts({ snapshot }: { snapshot: ReplaySnapshot }) {
  const data = snapshot.analysis;
  return (
    <div className="space-y-4">
      <h3 className="text-lg font-semibold">{data.projection_name} · analysis</h3>
      <dl className="grid gap-4 sm:grid-cols-2">
        {[
          ["Sampled events", data.sampled_events],
          ["Sampled entities", data.sampled_entity_count],
          ["Reported total", data.reported_total_events ?? "Unknown"],
          ["Current entities", data.current_entity_count ?? "Unknown"],
        ].map(([label, value]) => (
          <div key={label}>
            <dt className="text-muted-foreground">{label}</dt>
            <dd className="text-xl font-semibold">{value}</dd>
          </div>
        ))}
      </dl>
      <p>
        Analysis time: {new Date(data.analyzed_at).toLocaleString()}. At most 1,000 events were
        inspected.
      </p>
      <p>
        Unknown: total-count provenance, authoritative event order, restart proof, run comparison
        and archive completeness. A reported total does not prove complete history.
      </p>
    </div>
  );
}

export function CustomerReplayReport({
  review,
  busy,
  onDecision,
}: {
  review: ReplayReview;
  busy: boolean;
  onDecision: (decision: "approve" | "reject") => void;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    heading.current?.focus();
  }, []);
  const pending = review.effective_state === "pending" && review.expires_at * 1000 > Date.now();
  const resume =
    review.state === "approved" &&
    review.replay?.status === "not_started" &&
    review.expires_at * 1000 > Date.now();
  return (
    <section
      className="space-y-5 rounded-xl border p-5 sm:p-6"
      aria-labelledby="replay-decision-title"
    >
      <h2 ref={heading} tabIndex={-1} id="replay-decision-title" className="text-xl font-semibold">
        Projection rebuild review
      </h2>
      <p role="status">
        Review: {review.effective_state} · Version {review.version}
      </p>
      <p>Expires {expiryLabel(review.expires_at)}</p>
      {review.evidence && <ReplayFacts snapshot={review.evidence} />}
      {review.action && (
        <p>
          Approval allows one rebuild of <strong>{review.action.projection_name}</strong> from
          retained history available at dispatch, followed by live event catch-up. It replaces that
          projection’s read model. This analysis is not a frozen copy of all tenant events.
        </p>
      )}
      <details>
        <summary className="cursor-pointer">Exact version and content hash</summary>
        <p className="mt-3 break-all font-mono">Review {review.id}</p>
        <p className="break-all font-mono">{review.digest}</p>
        <p className="break-all font-mono">Operation {review.replay_operation_id}</p>
      </details>
      {review.decision && (
        <p className="break-words">
          Decision recorded for version {review.decision.version} by {review.decision.actor} (
          {review.decision.role}) at {expiryLabel(review.decision.at)}.
        </p>
      )}
      {review.state === "approved" && (
        <div className="space-y-2 rounded-lg bg-muted p-4">
          <h3 className="text-lg font-semibold">Execution: {review.replay?.status ?? "unknown"}</h3>
          <p>
            {review.replay?.status === "completed"
              ? `Rebuild completed. ${review.replay.processed_events ?? "Unknown number of"} ${review.replay.processed_events === 1 ? "event" : "events"} processed.`
              : review.replay?.status === "not_started"
                ? "Approval saved; no dispatch record exists. Resume uses the same approved version and operation after fresh checks."
                : review.replay?.status === "unknown"
                  ? "Dispatch or completion cannot be confirmed. This operation will not be dispatched again. Refresh to recover any durable result."
                  : "Approval alone does not confirm completion. Refresh the saved result to check progress."}
          </p>
          {review.replay?.replay_id && (
            <p className="break-all font-mono">{review.replay.replay_id}</p>
          )}
        </div>
      )}
      {pending && (
        <p>
          Only the current workspace administrator who owns this review can decide. Source changes
          require a new review version.
        </p>
      )}
      {(pending || resume) && (
        <div className="flex flex-wrap gap-3">
          <Button
            className="h-auto min-h-11 whitespace-normal text-base"
            disabled={busy}
            onClick={() => onDecision("approve")}
          >
            {resume ? "Resume approved replay" : `Approve version ${review.version} and rebuild`}
          </Button>
          {pending && (
            <Button
              className="h-auto min-h-11 whitespace-normal text-base"
              variant="outline"
              disabled={busy}
              onClick={() => onDecision("reject")}
            >
              Reject this version
            </Button>
          )}
        </div>
      )}
    </section>
  );
}
