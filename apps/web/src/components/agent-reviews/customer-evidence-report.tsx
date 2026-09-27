"use client";

import {
  type ChangeEvidence,
  type EvidenceReview,
  expiryLabel,
  type RunPin,
} from "@/lib/customer-review-client";

function Pin({ name, run }: { name: string; run: RunPin }) {
  return (
    <div className="min-w-0 space-y-2 rounded-lg bg-muted p-4">
      <h3 className="font-semibold">{name}</h3>
      <p className="break-all font-mono">{run.run_id}</p>
      <p>Revision {run.revision}</p>
      <p className="break-all font-mono">SHA-256: {run.digest}</p>
    </div>
  );
}

function Change({ name, value }: { name: string; value: ChangeEvidence | null }) {
  return (
    <div className="min-w-0 space-y-3">
      <h4 className="font-semibold">{name}</h4>
      {!value ? (
        <p>No recorded change.</p>
      ) : (
        <>
          <p>Recorded change state: {value.state.replaceAll("_", " ")}</p>
          <p className="break-all font-mono">Evidence SHA-256: {value.evidence_sha256}</p>
          <h5 className="font-semibold">Attempts</h5>
          {value.attempts.length === 0 ? (
            <p>No recorded attempts.</p>
          ) : (
            <ol className="list-decimal space-y-3 pl-6">
              {value.attempts.map((attempt, index) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: immutable ordered history; repeated identical attempts have no separate IDs in this report.
                <li key={`${index}:${attempt.test_sha256}`}>
                  <p>
                    {attempt.state} · Test: {attempt.test_outcome ?? "unknown"}
                  </p>
                  <p className="break-all font-mono">
                    Test SHA-256: {attempt.test_sha256 ?? "not recorded"}
                  </p>
                </li>
              ))}
            </ol>
          )}
          <details>
            <summary className="cursor-pointer font-semibold">
              Recorded evidence sequence ({value.evidence.length})
            </summary>
            <ol className="mt-3 list-decimal space-y-3 pl-6">
              {value.evidence.map((event, index) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: immutable evidence sequence; position distinguishes repeated identical facts.
                <li key={`${index}:${event.kind}`}>
                  <p>
                    {event.kind}
                    {event.outcome ? ` · ${event.outcome}` : ""}
                  </p>
                  {event.evidence_sha256 && (
                    <p className="break-all font-mono">SHA-256: {event.evidence_sha256}</p>
                  )}
                </li>
              ))}
            </ol>
          </details>
        </>
      )}
    </div>
  );
}

export function CustomerEvidenceReport({ review }: { review: EvidenceReview }) {
  const report = review.state === "pending" ? review.evidence : undefined;
  return (
    <section
      className="min-w-0 space-y-5 rounded-xl border p-5 sm:p-6"
      aria-labelledby="comparison-title"
    >
      <h2 id="comparison-title" className="text-xl font-semibold">
        Comparison evidence
      </h2>
      <p className="rounded-lg bg-muted p-4 font-semibold">
        {report ? "Pending human review" : `Evidence ${review.state}`} · Unapproved · No action
        executed
      </p>
      <p>
        Comparison describes recorded evidence. It grants no replay or infrastructure authority.
      </p>
      <dl className="space-y-3">
        <div>
          <dt className="font-semibold">Review ID · version {review.version}</dt>
          <dd className="break-all font-mono">{review.id}</dd>
        </div>
        <div>
          <dt className="font-semibold">Review digest</dt>
          <dd className="break-all font-mono">{review.digest}</dd>
        </div>
        <div>
          <dt className="font-semibold">Expires</dt>
          <dd>{expiryLabel(review.expires_at)}</dd>
        </div>
      </dl>
      {!report ? (
        <p role="status">
          Evidence {review.state}. Refresh saved work and inspect the current sources before
          preparing another comparison.
        </p>
      ) : (
        <>
          <p className="font-semibold">{report.state.replaceAll("_", " ")}</p>
          <p>
            Checked when opened. Use “Open review” again to recheck access and evidence; each new
            check uses 2 queries.
          </p>
          <div className="grid gap-4 lg:grid-cols-2">
            <Pin name="Baseline" run={report.baseline} />
            <Pin name="Candidate" run={report.candidate} />
          </div>
          <div className="space-y-2">
            <h3 className="font-semibold">Uncertainty</h3>
            <p>
              {report.unknowns.length
                ? report.unknowns.join(", ")
                : "No capture gaps reported. Recorded evidence does not establish completeness beyond these runs."}
            </p>
            <p>
              Changed descriptors:{" "}
              {report.descriptor_changes.length
                ? report.descriptor_changes.join(", ")
                : "none recorded"}
            </p>
            <p>
              First divergence:{" "}
              {report.first_divergence
                ? `change ${report.first_divergence.change_number}`
                : "none recorded"}
            </p>
          </div>
          {report.differences.length === 0 ? (
            <p>No differences in recorded changes.</p>
          ) : (
            report.differences.map((difference) => (
              <section key={difference.change_number} className="space-y-4 border-t pt-5">
                <h3 className="text-xl font-semibold">Change {difference.change_number}</h3>
                <div className="grid gap-6 lg:grid-cols-2">
                  <Change name="Baseline" value={difference.baseline} />
                  <Change name="Candidate" value={difference.candidate} />
                </div>
              </section>
            ))
          )}
        </>
      )}
    </section>
  );
}
