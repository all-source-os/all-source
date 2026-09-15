"use client";

import useSWR from "swr";
import { useTimeTravelOptional } from "@/hooks/use-time-travel";
import { apiClient } from "@/lib/api/client";

export interface IngestionPoint {
  timestamp: string;
  count: number;
}

interface DashboardStats {
  events: {
    /** Period usage from the billing meter (`/api/tenant/usage`). Drives the QUOTA bar. */
    used: number;
    quota: number;
    percentage: number;
    /**
     * REAL tenant-scoped total events in the store, summed from per-event-type
     * counts (`/api/event-types`). This is the honest "how much is in here" number
     * and is what the Total-Events / Instance-Stats displays read — NOT the meter,
     * which is 0 for tenants whose data was ingested outside the metered QS path.
     */
    total: number;
  };
  queries: {
    used: number;
    quota: number;
    percentage: number;
  };
  /** Hosted extraction LLM tokens vs the tier allowance (-1 = unlimited, 0 = none). */
  extraction: {
    used: number;
    quota: number;
  };
  projections: {
    count: number;
    active: number;
  };
  /** Tenant-scoped distinct counts (real, from the event store via the QS). */
  store: {
    streams: number;
    eventTypes: number;
  };
  latency: {
    p99_us: number;
    formatted: string;
  };
  storage: {
    bytes: number;
    formatted: string;
  };
  /** Real daily event-ingestion series for the 30-day Overview chart. */
  ingestion: IngestionPoint[];
  /**
   * Real daily query series for the 30-day Overview chart. Per-tenant and
   * honest: empty until reads are event-sourced (see the QS analytics
   * `query_rate`); never a fabricated trend.
   */
  queries_series: IngestionPoint[];
  isHistorical?: boolean;
  asOf?: string | null;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const val = bytes / 1024 ** i;
  return `${val < 10 ? val.toFixed(1) : Math.round(val)} ${units[i]}`;
}

function formatLatency(us: number): string {
  if (us < 1000) return `${us.toFixed(1)}μs`;
  return `${(us / 1000).toFixed(1)}ms`;
}

const DEFAULT_STATS: DashboardStats = {
  events: { used: 0, quota: 5000000, percentage: 0, total: 0 },
  queries: { used: 0, quota: 500000, percentage: 0 },
  extraction: { used: 0, quota: 0 },
  projections: { count: 0, active: 0 },
  store: { streams: 0, eventTypes: 0 },
  latency: { p99_us: 0, formatted: "—" },
  storage: { bytes: 0, formatted: "—" },
  ingestion: [],
  queries_series: [],
};

// Many list endpoints are unwrapped by apiClient.request() (`{data:X}` → X), so a
// response may arrive as a bare array or as the `{data, total, count}` wrapper.
// Normalize to the array form regardless of which shape we got.
function asArray<T>(raw: unknown): T[] {
  if (Array.isArray(raw)) return raw as T[];
  const data = (raw as { data?: unknown } | undefined)?.data;
  return Array.isArray(data) ? (data as T[]) : [];
}

function totalFrom(raw: unknown, list: unknown[]): number {
  if (Array.isArray(raw)) return raw.length;
  return (raw as { total?: number } | undefined)?.total ?? list.length;
}

async function fetchDashboardStats(asOfIso: string | null): Promise<DashboardStats> {
  // Set the asOf on the client for time travel queries
  apiClient.setAsOf(asOfIso);

  // Fetch all data in parallel. Event counts/streams/ingestion come from the REAL
  // event store (Core via the Query Service); usage is the billing meter.
  const [
    usageResponse,
    projectionsResponse,
    metricsResponse,
    eventTypesResponse,
    streamsResponse,
    analyticsResponse,
  ] = await Promise.all([
    apiClient.getTenantUsage(),
    apiClient.listProjections(),
    apiClient.getMetrics(),
    // limit high enough to enumerate every type so the summed count is the true total
    apiClient.listEventTypes({ limit: 1000 }),
    apiClient.listStreams({ limit: 1 }),
    apiClient.getUsageAnalytics({ range: "30d" }),
  ]);

  // Build stats object with fallbacks
  const stats: DashboardStats = {
    ...DEFAULT_STATS,
    events: { ...DEFAULT_STATS.events },
    queries: { ...DEFAULT_STATS.queries },
    extraction: { ...DEFAULT_STATS.extraction },
    projections: { ...DEFAULT_STATS.projections },
    store: { ...DEFAULT_STATS.store },
    latency: { ...DEFAULT_STATS.latency },
    storage: { ...DEFAULT_STATS.storage },
    ingestion: [],
    queries_series: [],
  };

  // Usage meter (period usage → quota bars)
  if (usageResponse.data) {
    stats.events.used = usageResponse.data.events.used;
    stats.events.quota = usageResponse.data.events.quota;
    stats.events.percentage = usageResponse.data.events.percentage;
    stats.queries = usageResponse.data.queries;
    if (usageResponse.data.extraction_tokens) {
      stats.extraction = {
        used: usageResponse.data.extraction_tokens.used,
        quota: usageResponse.data.extraction_tokens.quota,
      };
    }
  }

  // REAL tenant-scoped totals from the event store.
  // /api/event-types returns each distinct type with its real `event_count`;
  // summing them yields the true total events held for this tenant — the honest
  // "Total Events" number (the meter above is 0 for out-of-band ingestion).
  {
    const types = asArray<{ event_count?: number }>(eventTypesResponse.data);
    stats.store.eventTypes = totalFrom(eventTypesResponse.data, types);
    const summed = types.reduce((sum, t) => sum + (Number(t.event_count) || 0), 0);
    // Prefer the real store count; fall back to the meter only if the store query
    // returned nothing (never invent a number).
    stats.events.total = summed > 0 ? summed : stats.events.used;

    // The billing meter can drift ABOVE the real store count (out-of-band ingest
    // or a backfill). "events used > events that exist" is impossible — clamp the
    // displayed usage to the real total so the bar is always sensible.
    if (stats.events.total > 0 && stats.events.used > stats.events.total) {
      stats.events.used = stats.events.total;
      stats.events.percentage =
        stats.events.quota > 0 ? (stats.events.used / stats.events.quota) * 100 : 0;
    }
  }

  {
    const streams = asArray<unknown>(streamsResponse.data);
    stats.store.streams = totalFrom(streamsResponse.data, streams);
  }

  // Real daily ingestion series for the 30-day chart.
  if (analyticsResponse.data?.ingestion_rate) {
    stats.ingestion = analyticsResponse.data.ingestion_rate.map((p) => ({
      timestamp: p.timestamp,
      count: p.count,
    }));
  }

  // Real daily query series for the 30-day chart. Per-tenant; honest-empty
  // until reads are event-sourced (the QS returns [] in that case).
  if (analyticsResponse.data?.query_rate) {
    stats.queries_series = analyticsResponse.data.query_rate.map((p) => ({
      timestamp: p.timestamp,
      count: p.count,
    }));
  }

  // Per-tenant enabled projections. QS returns {projections, total} scoped to
  // THIS tenant (not the global engine registry that used to leak here). "active"
  // = projections whose background backfill has finished (status "ready").
  if (projectionsResponse.data) {
    const { projections, total } = projectionsResponse.data;
    const list = projections ?? [];
    stats.projections = {
      count: total ?? list.length,
      active: list.filter((p) => p.status === "ready").length,
    };
  }

  // Real PLATFORM/SYSTEM metrics from the backend. The QS `/api/metrics`
  // controller now parses Core's Prometheus exposition into a typed `backend`
  // summary (see QueryServiceEx.CoreBackendMetrics), so these reads resolve:
  //   - p99_latency_us: the 0.99 quantile of Core's query-duration histogram,
  //     across ALL query types and ALL tenants. A platform latency property —
  //     honest to show, labelled as system/platform (NOT "your" latency).
  //   - storage_bytes: Core's on-disk data-dir size (every tenant's Parquet+WAL).
  //     Platform-wide; reads 0 until the Core gauge-population change ships, in
  //     which case we render an honest "—" rather than a fake number.
  // Both fall back to an honest "—" when null/0/absent — never fabricated.
  if (metricsResponse.data) {
    const backend = (metricsResponse.data as unknown as Record<string, unknown>).backend as
      | Record<string, unknown>
      | undefined;
    const p99 = (backend?.p99_latency_us as number) ?? (backend?.latency_p99_us as number) ?? 0;
    stats.latency = { p99_us: p99, formatted: p99 > 0 ? formatLatency(p99) : "—" };

    const storageBytes =
      (backend?.storage_bytes as number) ?? (backend?.disk_usage_bytes as number) ?? 0;
    stats.storage = {
      bytes: storageBytes,
      formatted: storageBytes > 0 ? formatBytes(storageBytes) : "—",
    };
  }

  // Include time travel metadata
  stats.isHistorical = asOfIso !== null;
  stats.asOf = asOfIso;

  return stats;
}

export function useDashboardStats() {
  const { asOfIso, isHistorical } = useTimeTravelOptional();

  const { data, error, isLoading, mutate } = useSWR(
    // Include asOfIso in the key so SWR refetches when time travel changes
    asOfIso ? `/dashboard/stats?as_of=${asOfIso}` : "/dashboard/stats",
    () => fetchDashboardStats(asOfIso),
    {
      revalidateOnFocus: false,
      dedupingInterval: 30000, // Cache for 30 seconds
      fallbackData: DEFAULT_STATS,
      onError: (err) => {
        console.warn("Failed to fetch dashboard stats:", err.message);
      },
    }
  );

  return {
    stats: data || DEFAULT_STATS,
    isLoading,
    isHistorical,
    error: error?.message,
    refresh: () => mutate(),
  };
}
