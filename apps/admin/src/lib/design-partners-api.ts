export type DesignPartnerStatus = "new" | "reviewing" | "accepted" | "waitlisted" | "rejected";

export interface DesignPartnerCampaignSource {
  source?: string;
  medium?: string;
  campaign?: string;
  content?: string;
  term?: string;
}

export interface DesignPartnerStatusChange {
  status: DesignPartnerStatus;
  changed_at: string;
  actor?: string;
  note?: string;
}

export interface DesignPartnerApplication {
  id: string;
  name: string;
  email: string;
  project: string;
  agent_use_case: string;
  memory_problem: string;
  timeline: string;
  status: DesignPartnerStatus;
  submitted_at: string;
  consent_version: string;
  campaign_source: DesignPartnerCampaignSource;
  retention_until?: string;
  status_history: DesignPartnerStatusChange[];
}

export class DesignPartnerApiError extends Error {
  status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = "DesignPartnerApiError";
    this.status = status;
  }
}

async function readError(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as { message?: string; error?: string };
    return body.message || body.error || `Request failed (${response.status})`;
  } catch {
    return `Request failed (${response.status})`;
  }
}

export async function fetchDesignPartnerApplications(
  status?: DesignPartnerStatus
): Promise<DesignPartnerApplication[]> {
  const query = status ? `?status=${encodeURIComponent(status)}` : "";
  const response = await fetch(`/api/v1/admin/design-partners/applications${query}`, {
    credentials: "include",
    cache: "no-store",
  });
  if (!response.ok) {
    throw new DesignPartnerApiError(await readError(response), response.status);
  }
  const body = (await response.json()) as { applications?: DesignPartnerApplication[] };
  return Array.isArray(body.applications) ? body.applications : [];
}

export async function updateDesignPartnerStatus(
  applicationID: string,
  status: DesignPartnerStatus,
  note: string
): Promise<DesignPartnerApplication> {
  const response = await fetch(
    `/api/v1/admin/design-partners/applications/${encodeURIComponent(applicationID)}/status`,
    {
      method: "PUT",
      credentials: "include",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ status, note }),
    }
  );
  if (!response.ok) {
    throw new DesignPartnerApiError(await readError(response), response.status);
  }
  return response.json();
}
