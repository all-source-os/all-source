export type EvidenceConnection = {
  id: string;
  client_id: string;
  expires_at: number;
  status: string;
  consent?: { version: string };
};
export type SourceReference = {
  kind: "run_evidence";
  ref: string;
  revision: number;
  sha256: string;
};
export type SavedSource = {
  source: SourceReference;
  run_id: string;
  expires_at: number;
  status: string;
};
export type SavedReview = {
  id: string;
  version: number;
  digest: string;
  expires_at: number;
  status: string;
};
export type EvidenceWorkspace = {
  sources: SavedSource[];
  reviews: SavedReview[];
  connection_status: string;
};
export type RunInspection = {
  run_id: string;
  revision: number;
  sha256: string;
  changes: number;
  attempts: number;
  completed: boolean;
  unknowns: string[];
  shared: false;
};
export type RunPin = { run_id: string; revision: number; digest: string };
export type ChangeEvidence = {
  state: string;
  evidence_sha256: string;
  attempts: { state: string; test_outcome: string | null; test_sha256: string | null }[];
  evidence: { kind: string; evidence_sha256?: string; outcome?: string }[];
};
export type ComparisonEvidence = {
  state: string;
  baseline: RunPin;
  candidate: RunPin;
  descriptor_changes: string[];
  first_divergence: { change_number: number } | null;
  differences: {
    change_number: number;
    baseline: ChangeEvidence | null;
    candidate: ChangeEvidence | null;
  }[];
  unknowns: string[];
  execution: "none";
  approval_authority: "not_established";
};
export type EvidenceReview = {
  id: string;
  version: number;
  digest: string;
  expires_at: number;
  state: "pending" | "expired" | "superseded" | "unavailable";
  approved: false;
  execution: "none";
  evidence?: ComparisonEvidence;
};

export const operationId = () => `${Math.floor(Date.now() / 1000)}:${crypto.randomUUID()}`;
export const expiryLabel = (seconds: number) => new Date(seconds * 1000).toLocaleString();

export async function customerRequest<T>(operation: string, body: unknown): Promise<T> {
  const response = await fetch(`/api/customer-agent/connections/${operation}`, {
    method: "POST",
    credentials: "same-origin",
    cache: "no-store",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(50_000),
  });
  const result = await response.json();
  if (!response.ok)
    throw new Error(
      result.error?.message || "Request unavailable. Retry the same request or refresh saved work."
    );
  return result.data as T;
}
