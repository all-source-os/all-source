import type { SourceReference } from "./customer-review-client";

export type ReplaySnapshot = {
  sample_sha256: string;
  catalog_sha256: string;
  analysis: {
    projection_name: string;
    projection_status: string | null;
    analyzed_at: string;
    sampled_events: number;
    sampled_entity_count: number;
    current_entity_count: number | null;
    reported_total_events: number | null;
    analysis_scope: string | null;
    unknowns: string[];
  };
};
export type ReplayProposal = {
  schema_version: 1;
  kind: "replay_plan";
  projection_name: string;
  sources: SourceReference[];
};
export type ReplayReview = {
  id: string;
  digest: string;
  version: number;
  expires_at: number;
  state: "pending" | "approved" | "rejected";
  effective_state: string;
  replay_operation_id: string;
  proposal?: ReplayProposal;
  evidence?: ReplaySnapshot;
  action?: { projection_name: string; history: "retained_at_dispatch"; live_catchup: true };
  decision?: {
    id: string;
    actor: string;
    role: string;
    at: number;
    digest: string;
    version: number;
  } | null;
  replay?: {
    status: string;
    replay_id?: string;
    processed_events?: number | null;
    cutoff?: string;
  } | null;
};

export async function replayRequest<T>(
  operation: string,
  connection: string,
  input?: unknown
): Promise<T> {
  const response = await fetch(`/api/customer-agent/replay/${operation}`, {
    method: "POST",
    credentials: "same-origin",
    cache: "no-store",
    headers: { "content-type": "application/json" },
    signal: AbortSignal.timeout(50_000),
    body: JSON.stringify({ connection_id: connection, ...(input === undefined ? {} : { input }) }),
  });
  const result = await response.json();
  if (!response.ok)
    throw new Error(
      result.error?.message ||
        "Replay request unavailable. Refresh the saved result before retrying."
    );
  return result.data as T;
}
