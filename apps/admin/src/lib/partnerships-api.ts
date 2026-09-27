export const partnershipStatuses = {
  research: "Research",
  ready: "Ready for review",
  awaiting_reply: "Awaiting reply",
  engaged: "Conversation",
  pilot: "Pilot",
  won: "Commercial outcome",
  parked: "Parked",
  do_not_contact: "Do not contact",
} as const;
export const partnershipKinds = {
  vc: "VC",
  family_office: "Family office",
  accelerator: "Accelerator",
  corporate: "Corporate",
  community: "Community",
  other: "Other",
} as const;
export interface PartnershipSource {
  url: string;
  title: string;
  evidence: string;
  checked_at: string;
}
export interface PartnershipScore {
  model: string;
  run_at: string;
  fit: number;
  leverage: number;
  access: number;
  paid_demand: number;
  rationale: string;
}
export interface PartnershipMessage {
  id: string;
  channel: "email" | "linkedin" | "x" | "form" | "other";
  direction: "outbound" | "inbound";
  outcome: "draft" | "sent" | "received" | "failed" | "unknown";
  destination: string;
  subject: string;
  body: string;
  occurred_at: string;
  verification: string;
  approval_note: string;
}
export interface PartnershipRecord {
  id: string;
  organization: string;
  kind: keyof typeof partnershipKinds;
  geography: string;
  website: string;
  status: keyof typeof partnershipStatuses;
  angle: string;
  contact_route: string;
  next_action: string;
  next_action_at: string;
  notes: string;
  limitations: string;
  reply_checked_at: string;
  sources: PartnershipSource[];
  score: PartnershipScore | null;
  messages: PartnershipMessage[];
}
export interface PartnershipRevision {
  record: PartnershipRecord;
  revision: number;
  saved_at: string;
  actor: string;
}
export function emptyPartnership(): PartnershipRecord {
  return {
    id: "",
    organization: "",
    kind: "vc",
    geography: "",
    website: "",
    status: "research",
    angle: "",
    contact_route: "",
    next_action: "",
    next_action_at: "",
    notes: "",
    limitations: "",
    reply_checked_at: "",
    sources: [],
    score: null,
    messages: [],
  };
}
export function safePartnershipURL(value: string): string | undefined {
  try {
    const url = new URL(value);
    return ["https:", "http:"].includes(url.protocol) && !url.username && !url.password
      ? url.href
      : undefined;
  } catch {
    return undefined;
  }
}
export function partnershipPriority(record: PartnershipRecord): number | null {
  return record.score ? record.score.fit + record.score.leverage + record.score.access : null;
}
export function partnershipStats(records: PartnershipRevision[]): {
  researched: number;
  sent: number;
  replies: number;
  unchecked: number;
} {
  return {
    researched: records.length,
    sent: records.filter(({ record }) => record.messages.some((m) => m.outcome === "sent")).length,
    replies: records.filter(({ record }) => record.messages.some((m) => m.outcome === "received"))
      .length,
    unchecked: records.filter(
      ({ record }) => record.messages.some((m) => m.outcome === "sent") && !record.reply_checked_at
    ).length,
  };
}
export function partnershipPrompt(id?: string): string {
  return `Use $allsource-partnerships in this AllSource monorepo. ${id ? `Review organisation ${id}` : "Review the Partnerships pipeline"} in the authenticated admin dashboard. Read existing records and sent history first. Refresh public evidence and distinguish facts, inferences and unknowns. If Jev is available, preserve its dated rubric and raw evidence; do not invent a score. Draft one relevant commercial enquiry as AllSource's founder, not a fundraising pitch. Show exact recipient, channel and message and ask for current approval before any send. After an authorised send, verify the outcome and persist the exact message, timestamp and proof in Partnerships. Never auto-follow up or resend an uncertain send.`;
}

// Import is create-only; server is the authority for field validation. This
// check prevents silent truncation and accidental use of an unrelated JSON file.
export function parsePartnershipImport(text: string): PartnershipRecord[] {
  if (text.length > 4_000_000) throw new Error("Import exceeds 4 MB.");
  const data: unknown = JSON.parse(text);
  if (
    !data ||
    typeof data !== "object" ||
    !("records" in data) ||
    !Array.isArray(data.records) ||
    data.records.length < 1 ||
    data.records.length > 100
  )
    throw new Error("Use { records: [...] } with 1–100 partnership records.");
  const seen = new Set<string>();
  for (const r of data.records) {
    if (
      !r ||
      typeof r !== "object" ||
      typeof r.organization !== "string" ||
      typeof r.website !== "string" ||
      !safePartnershipURL(r.website)
    )
      throw new Error("Each record needs an organisation and valid website.");
    const id = new URL(r.website).hostname.toLowerCase().replace(/^www\./, "");
    if (seen.has(id)) throw new Error(`Duplicate organisation hostname: ${id}`);
    seen.add(id);
  }
  return data.records as PartnershipRecord[];
}
async function request<T>(path = "", body?: unknown): Promise<T> {
  const response = await fetch(`/api/v1/admin/partnerships${path}`, {
    method: body ? "PUT" : "GET",
    credentials: "include",
    cache: "no-store",
    headers: { "content-type": "application/json" },
    ...(body ? { body: JSON.stringify(body) } : {}),
  });
  if (!response.ok) {
    const error = await response.json().catch(() => null);
    throw new Error(
      typeof error?.message === "string" ? error.message : `Request failed (${response.status}).`
    );
  }
  return response.json() as Promise<T>;
}
export async function fetchPartnerships(): Promise<PartnershipRevision[]> {
  const data = await request<{ records: PartnershipRevision[] }>();
  if (!Array.isArray(data.records))
    throw new Error("Partnership response is incomplete. Retry; no records were changed.");
  return data.records;
}
export async function savePartnership(
  record: PartnershipRecord,
  revision: number
): Promise<PartnershipRevision> {
  return request("", { record, expected_revision: revision });
}
export async function fetchPartnershipHistory(id: string): Promise<PartnershipRevision[]> {
  const data = await request<{ history: PartnershipRevision[] }>(
    `/${encodeURIComponent(id)}/history`
  );
  if (!Array.isArray(data.history)) throw new Error("History response is incomplete.");
  return data.history;
}
