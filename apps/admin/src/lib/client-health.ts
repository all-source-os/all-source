import type { ClientHealthNow, TenantHealth, TenantState } from "./metrics-api";

export const WARN_SERVED_RATIO = 0.95;
export const BAD_SERVED_RATIO = 0.8;
/** A named tenant averaging at least this many requests/min is expected to be sending now. */
export const EXPECTED_MIN_PER_MIN = 10;

export const ANONYMOUS_TENANT = "anonymous";

export type Tone = "ok" | "warn" | "bad" | "unknown";

export interface Verdict {
  tone: Tone;
  text: string;
}

const fmt = (n: number) => n.toLocaleString("en-US");

function windowLabel(now: ClientHealthNow): string {
  return `${Math.round((now.window_seconds || 300) / 60)} min`;
}

/** Floors to one decimal so 99.96% with requests refused never reads as 100.0%. */
export function formatServedPercent(ratio: number): string {
  return `${(Math.floor(ratio * 1000) / 10).toFixed(1)}%`;
}

function dominantReason(now: ClientHealthNow): string {
  const other4xx = now.total - now.served - now.refused_auth - now.throttled - now.failed;
  const reasons: [string, number][] = [
    ["401/403", now.refused_auth],
    ["429", now.throttled],
    ["5xx", now.failed],
    ["other 4xx", other4xx],
  ];
  const top = reasons.filter(([, n]) => n > 0).sort((a, b) => b[1] - a[1])[0];
  return top ? top[0] : "other 4xx";
}

function expectedPerMin(tenants: TenantHealth[]): number {
  return tenants
    .filter((t) => t.tenant !== ANONYMOUS_TENANT)
    .reduce((sum, t) => sum + (Number.isFinite(t.baseline_per_min) ? t.baseline_per_min : 0), 0);
}

export function verdictFor(
  now: ClientHealthNow | null | undefined,
  tenants: TenantHealth[] = []
): Verdict {
  if (!now) return { tone: "unknown", text: "Client traffic not measured." };
  const window = windowLabel(now);
  if (now.total === 0 || now.served_ratio === null) {
    const expected = expectedPerMin(tenants);
    if (expected >= EXPECTED_MIN_PER_MIN) {
      return {
        tone: "bad",
        text: `No client requests reached the control plane in the last ${window} (expected ~${fmt(Math.round(expected))}/min).`,
      };
    }
    return { tone: "unknown", text: `No client data requests in the last ${window}.` };
  }
  const ratio = now.served_ratio;
  if (ratio >= WARN_SERVED_RATIO) {
    return {
      tone: "ok",
      text: `Clients served: ${formatServedPercent(ratio)} of ${fmt(now.total)} requests in the last ${window}.`,
    };
  }
  const notServed = now.total - now.served;
  const tone = ratio < BAD_SERVED_RATIO ? "bad" : "warn";
  const reason = dominantReason(now);
  if (reason === "5xx") {
    return {
      tone,
      text: `Clients failing: ${fmt(notServed)} of ${fmt(now.total)} requests failed (5xx) in the last ${window}.`,
    };
  }
  return {
    tone,
    text: `Clients refused: ${fmt(notServed)} of ${fmt(now.total)} requests refused (${reason}) in the last ${window}.`,
  };
}

export function shareTone(part: number, total: number, warnAt: number, badAt: number): Tone {
  if (total === 0) return part > 0 ? "bad" : "ok";
  const share = part / total;
  if (share >= badAt) return "bad";
  if (share >= warnAt) return "warn";
  return "ok";
}

const STATE_RANK: Record<TenantState, number> = { quiet: 0, refused: 1, served: 2, idle: 3 };

export function orderTenants(tenants: TenantHealth[]): TenantHealth[] {
  return [...tenants].sort(
    (a, b) =>
      (STATE_RANK[a.state] ?? 4) - (STATE_RANK[b.state] ?? 4) ||
      b.served_5m - a.served_5m ||
      a.tenant.localeCompare(b.tenant)
  );
}

export function isAlarming(state: TenantState): boolean {
  return state === "quiet" || state === "refused";
}

export interface TenantCounts {
  served: number;
  quiet: number;
  refused: number;
}

/** Named tenants by state; anonymous is failed-auth traffic, not a tenant. */
export function countTenants(tenants: TenantHealth[]): TenantCounts {
  const named = tenants.filter((t) => t.tenant !== ANONYMOUS_TENANT);
  return {
    served: named.filter((t) => t.state === "served").length,
    quiet: named.filter((t) => t.state === "quiet").length,
    refused: named.filter((t) => t.state === "refused").length,
  };
}

export interface ReleaseGroup {
  ts: number;
  label: string;
  releases: { app: string; instance: string; ts: number }[];
}

export function shortApp(app: string): string {
  return app.replace(/^allsource-/, "");
}

/** Merges machine starts less than one step apart into one marker, e.g. "control-plane ×2, core". */
export function groupReleases(
  releases: { app: string; instance: string; started_at: string }[],
  stepMs: number
): ReleaseGroup[] {
  const sorted = releases
    .map((r) => ({ app: r.app, instance: r.instance, ts: Date.parse(r.started_at) }))
    .filter((r) => Number.isFinite(r.ts))
    .sort((a, b) => a.ts - b.ts);
  const groups: ReleaseGroup[] = [];
  for (const r of sorted) {
    const g = groups[groups.length - 1];
    if (g && r.ts - g.ts < stepMs) g.releases.push(r);
    else groups.push({ ts: r.ts, label: "", releases: [r] });
  }
  for (const g of groups) {
    const byApp = new Map<string, number>();
    for (const r of g.releases) byApp.set(shortApp(r.app), (byApp.get(shortApp(r.app)) ?? 0) + 1);
    g.label = [...byApp].map(([app, n]) => (n > 1 ? `${app} ×${n}` : app)).join(", ");
  }
  return groups;
}
