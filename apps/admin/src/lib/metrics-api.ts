/**
 * Metrics API client for the monitoring dashboard.
 *
 * Calls the Control Plane admin metrics endpoints SAME-ORIGIN via the BFF proxy
 * (src/app/api/v1/[...path]/route.ts), which attaches the admin_token Bearer:
 *   GET /api/v1/admin/metrics/client-health?range=<range>  (Fly Prometheus)
 *   GET /api/v1/admin/metrics/summary                      (Core internals via QS)
 *
 * A cross-origin call to the CP or QS never authenticates (CONTROL_PLANE_CORS.md
 * §4). A missing value stays null: it renders as "Not measured", never as 0.
 */

function getApiUrl(): string {
  if (typeof window !== "undefined") {
    return "";
  }
  return process.env.NEXT_PUBLIC_API_URL || "http://localhost:3902";
}

export type TimeRange = "1h" | "24h" | "7d";

export interface MetricsSummary {
  events_total: number | null;
  query_latency_p99_ms: number | null;
}

export interface ClientHealthNow {
  window_seconds: number;
  served: number;
  refused_auth: number;
  throttled: number;
  failed: number;
  total: number;
  served_ratio: number | null;
}

export interface ClientHealthPoint {
  t: string;
  served: number;
  refused_auth: number;
  throttled: number;
  failed: number;
}

export type TenantState = "served" | "quiet" | "refused" | "idle";

/** Counts cover the last 5 minutes; baseline_per_min is the 24h average before that. */
export interface TenantHealth {
  tenant: string;
  served_5m: number;
  refused_5m: number;
  failed_5m: number;
  baseline_per_min: number;
  state: TenantState;
}

export interface Release {
  app: string;
  instance: string;
  started_at: string;
}

export interface Machine {
  app: string;
  instance: string;
  up: boolean;
  started_at: string;
  uptime_seconds: number;
}

export interface ClientHealthReport {
  configured: boolean;
  range: TimeRange;
  step_seconds?: number;
  generated_at: string;
  now: ClientHealthNow | null;
  series: ClientHealthPoint[];
  tenants: TenantHealth[];
  releases: Release[];
  machines: Machine[];
}

export type ClientHealthResult =
  | { kind: "ok"; report: ClientHealthReport }
  | { kind: "not_configured" }
  | { kind: "error"; message: string };

function finiteOrNull(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

function listOf<T>(v: unknown): T[] {
  return Array.isArray(v) ? (v as T[]) : [];
}

const NOW_COUNTS = [
  "window_seconds",
  "served",
  "refused_auth",
  "throttled",
  "failed",
  "total",
] as const;

function parseNow(v: unknown): ClientHealthNow | null {
  if (!v || typeof v !== "object") return null;
  const raw = v as Record<string, unknown>;
  const out: Partial<Record<(typeof NOW_COUNTS)[number], number>> = {};
  for (const key of NOW_COUNTS) {
    const n = finiteOrNull(raw[key]);
    if (n === null) return null;
    out[key] = n;
  }
  return {
    ...(out as Omit<ClientHealthNow, "served_ratio">),
    served_ratio: finiteOrNull(raw.served_ratio),
  };
}

export function parseClientHealth(data: unknown): ClientHealthReport {
  const d = (data && typeof data === "object" ? data : {}) as Record<string, unknown>;
  return {
    configured: d.configured === true,
    range: (d.range as TimeRange) ?? "1h",
    step_seconds: finiteOrNull(d.step_seconds) ?? undefined,
    generated_at: typeof d.generated_at === "string" ? d.generated_at : "",
    now: parseNow(d.now),
    series: listOf<ClientHealthPoint>(d.series),
    tenants: listOf<TenantHealth>(d.tenants),
    releases: listOf<Release>(d.releases),
    machines: listOf<Machine>(d.machines),
  };
}

export async function fetchClientHealth(range: TimeRange): Promise<ClientHealthResult> {
  let res: Response;
  try {
    res = await fetch(`${getApiUrl()}/api/v1/admin/metrics/client-health?range=${range}`, {
      credentials: "include",
    });
  } catch (e) {
    return {
      kind: "error",
      message: `Could not reach the control plane: ${e instanceof Error ? e.message : String(e)}`,
    };
  }
  const body: unknown = await res.json().catch(() => null);
  if (!res.ok) {
    if (res.status === 401 || res.status === 403) {
      return {
        kind: "error",
        message: `Your admin session isn't authorized for the Control Plane (HTTP ${res.status}). Try signing in again.`,
      };
    }
    const upstream =
      body && typeof body === "object" && typeof (body as { error?: unknown }).error === "string"
        ? (body as { error: string }).error
        : "";
    return {
      kind: "error",
      message:
        res.status === 502 && upstream
          ? `Prometheus query failed (HTTP 502): ${upstream}`
          : `Client health request failed (HTTP ${res.status})${upstream ? `: ${upstream}` : "."}`,
    };
  }
  const report = parseClientHealth(body);
  if (!report.configured) return { kind: "not_configured" };
  return { kind: "ok", report };
}

export async function fetchMetricsSummary(): Promise<MetricsSummary> {
  const res = await fetch(`${getApiUrl()}/api/v1/admin/metrics/summary`, {
    credentials: "include",
  });
  if (!res.ok) {
    throw new Error(`Failed to fetch metrics summary: ${res.status}`);
  }
  const data = ((await res.json()) ?? {}) as Record<string, unknown>;
  // The QS reports 0 for a series Core does not export, so 0 is "not measured"
  // here: Core serving traffic cannot have zero events or a 0 ms p99.
  const measured = (v: unknown) => {
    const n = finiteOrNull(v);
    return n !== null && n > 0 ? n : null;
  };
  return {
    events_total: measured(data.events_total),
    query_latency_p99_ms: measured(data.query_latency_p99_ms),
  };
}

export function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);

  if (days > 0) {
    return `${days}d ${hours}h ${minutes}m`;
  }
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  return `${minutes}m`;
}
