"use client";

import { Button, Card, CardContent, CardDescription, CardHeader, CardTitle } from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import {
  AlertTriangle,
  Ban,
  Bell,
  CheckCircle2,
  Gauge,
  RefreshCw,
  ServerCrash,
  Target,
  Timer,
  Users,
} from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";
import { ClientHealthChart, RangeSelector } from "@/components/monitoring/client-health-chart";
import { MachinesPanel } from "@/components/monitoring/machines-panel";
import { StatCard } from "@/components/monitoring/stat-card";
import { TenantHealthTable } from "@/components/monitoring/tenant-health-table";
import {
  countTenants,
  formatServedPercent,
  shareTone,
  type Tone,
  verdictFor,
} from "@/lib/client-health";
import {
  type ClientHealthResult,
  fetchClientHealth,
  fetchMetricsSummary,
  type MetricsSummary,
  type TimeRange,
} from "@/lib/metrics-api";

const AUTO_REFRESH_INTERVAL = 30_000;

type HealthState = { kind: "loading" } | ClientHealthResult;
type SummaryState =
  | { kind: "loading" }
  | { kind: "ok"; summary: MetricsSummary }
  | { kind: "error"; message: string };

const TONE_STATUS: Record<Tone, "healthy" | "warning" | "critical" | "unknown"> = {
  ok: "healthy",
  warn: "warning",
  bad: "critical",
  unknown: "unknown",
};

const fmt = (n: number) => n.toLocaleString("en-US");

export default function MonitoringPage() {
  const [health, setHealth] = useState<HealthState>({ kind: "loading" });
  const [summary, setSummary] = useState<SummaryState>({ kind: "loading" });
  const [range, setRange] = useState<TimeRange>("1h");
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [secondsUntilRefresh, setSecondsUntilRefresh] = useState(AUTO_REFRESH_INTERVAL / 1000);
  const rangeRef = useRef(range);
  rangeRef.current = range;

  const load = useCallback(async (requested: TimeRange) => {
    setIsRefreshing(true);
    const [h, s] = await Promise.all([
      fetchClientHealth(requested),
      fetchMetricsSummary().then(
        (summary): SummaryState => ({ kind: "ok", summary }),
        (e): SummaryState => ({
          kind: "error",
          message: e instanceof Error ? e.message : String(e),
        })
      ),
    ]);
    if (rangeRef.current === requested) setHealth(h);
    setSummary(s);
    setIsRefreshing(false);
    setSecondsUntilRefresh(AUTO_REFRESH_INTERVAL / 1000);
  }, []);

  const refresh = useCallback(() => load(rangeRef.current), [load]);

  const selectRange = useCallback((next: TimeRange) => {
    if (next === rangeRef.current) return;
    setHealth({ kind: "loading" });
    setRange(next);
  }, []);

  useEffect(() => {
    load(range);
  }, [range, load]);

  useEffect(() => {
    const timer = setInterval(refresh, AUTO_REFRESH_INTERVAL);
    const countdown = setInterval(
      () => setSecondsUntilRefresh((prev) => Math.max(0, prev - 1)),
      1000
    );
    return () => {
      clearInterval(timer);
      clearInterval(countdown);
    };
  }, [refresh]);

  const report = health.kind === "ok" ? health.report : null;
  const now = report?.now ?? null;
  const tenants = report?.tenants ?? [];
  const verdict =
    health.kind === "ok"
      ? verdictFor(now, tenants)
      : health.kind === "not_configured"
        ? {
            tone: "unknown" as const,
            text: "Client traffic not measured: Prometheus is not configured.",
          }
        : health.kind === "error"
          ? { tone: "unknown" as const, text: "Client traffic unknown: the metrics query failed." }
          : { tone: "unknown" as const, text: "Checking client traffic…" };

  const unmeasured =
    health.kind === "loading" ? "Loading…" : health.kind === "error" ? "Error" : "Not measured";
  const tenantCounts = countTenants(tenants);

  return (
    <div className="min-w-0 space-y-6" data-testid="monitoring-page">
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Monitoring</h1>
          <p className="text-muted-foreground">
            Are clients being served right now, and who is being refused?
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <RangeSelector range={range} onRangeChange={selectRange} />
          <div
            className="flex items-center gap-2 text-xs text-muted-foreground"
            data-testid="auto-refresh-indicator"
          >
            <div
              className={cn(
                "h-2 w-2 rounded-full",
                isRefreshing ? "animate-pulse bg-yellow-500" : "bg-green-500"
              )}
            />
            <span>
              {isRefreshing ? "Refreshing..." : `Next refresh in ${secondsUntilRefresh}s`}
            </span>
          </div>
          <button
            type="button"
            onClick={refresh}
            className="min-h-9 min-w-9 rounded-md p-2 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
            aria-label="Refresh metrics"
          >
            <RefreshCw className={cn("h-4 w-4", isRefreshing && "animate-spin")} />
          </button>
        </div>
      </div>

      <div className="flex gap-3" data-testid="monitoring-subnav">
        <Link
          href="/monitoring/alerts"
          className="flex items-center gap-2 rounded-lg border border-border bg-card px-4 py-3 text-sm font-medium transition-colors hover:bg-muted"
          data-testid="nav-alerts"
        >
          <Bell className="h-4 w-4 text-muted-foreground" />
          Alert Rules
        </Link>
        <Link
          href="/monitoring/slos"
          className="flex items-center gap-2 rounded-lg border border-border bg-card px-4 py-3 text-sm font-medium transition-colors hover:bg-muted"
          data-testid="nav-slos"
        >
          <Target className="h-4 w-4 text-muted-foreground" />
          SLOs
        </Link>
      </div>

      <div
        role="status"
        data-testid="client-verdict"
        data-tone={verdict.tone}
        className={cn(
          "flex items-center gap-3 rounded-lg border p-4 text-lg font-semibold",
          verdict.tone === "ok" &&
            "border-green-500/40 bg-green-500/10 text-green-600 dark:text-green-400",
          verdict.tone === "warn" &&
            "border-yellow-500/40 bg-yellow-500/10 text-yellow-700 dark:text-yellow-400",
          verdict.tone === "bad" &&
            "border-red-500/40 bg-red-500/10 text-red-600 dark:text-red-400",
          verdict.tone === "unknown" && "border-border bg-muted text-muted-foreground"
        )}
      >
        {verdict.tone === "ok" ? (
          <CheckCircle2 className="h-5 w-5 shrink-0" />
        ) : (
          <AlertTriangle className="h-5 w-5 shrink-0" />
        )}
        <span>{verdict.text}</span>
      </div>

      {health.kind === "not_configured" && (
        <div className="rounded-md border p-4 text-sm" data-testid="client-health-not-configured">
          <p className="font-medium">
            Client health reads Fly managed Prometheus, and the control plane has no token for it.
          </p>
          <p className="mt-1 text-muted-foreground">
            Set <code className="font-mono">ALLSOURCE_METRICS_PROM_TOKEN</code> on the{" "}
            <code className="font-mono">allsource-control-plane</code> app to a read-only Fly token
            for the <code className="font-mono">allsource</code> org, as Fly prints it (
            <code className="font-mono">FlyV1 fm2_…</code>). A raw{" "}
            <code className="font-mono">fm2_</code> token also works; Fly rejects one sent as a
            Bearer token. <code className="font-mono">ALLSOURCE_METRICS_PROM_URL</code> overrides
            the default <code className="font-mono">https://api.fly.io/prometheus/allsource</code>.
          </p>
        </div>
      )}

      {health.kind === "error" && (
        <div
          role="alert"
          className="rounded-md border border-destructive p-4 text-sm"
          data-testid="client-health-error"
        >
          <p className="break-words">{health.message}</p>
          <Button className="mt-2" variant="outline" onClick={refresh}>
            Retry
          </Button>
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-5" data-testid="stat-cards">
        <StatCard
          title="Served"
          value={now ? fmt(now.served) : unmeasured}
          subtitle={
            now
              ? now.served_ratio === null
                ? "No requests in the last 5 min"
                : `${formatServedPercent(now.served_ratio)} of ${fmt(now.total)} in the last 5 min`
              : "2xx, last 5 min"
          }
          icon={CheckCircle2}
          status={now ? TONE_STATUS[verdict.tone] : "unknown"}
        />
        <StatCard
          title="Refused by auth"
          value={now ? fmt(now.refused_auth) : unmeasured}
          subtitle="401 and 403, last 5 min"
          icon={Ban}
          status={now ? TONE_STATUS[shareTone(now.refused_auth, now.total, 0.01, 0.2)] : "unknown"}
        />
        <StatCard
          title="Throttled"
          value={now ? fmt(now.throttled) : unmeasured}
          subtitle="429, last 5 min"
          icon={Timer}
          status={now ? TONE_STATUS[shareTone(now.throttled, now.total, 0.05, 0.2)] : "unknown"}
        />
        <StatCard
          title="Failed"
          value={now ? fmt(now.failed) : unmeasured}
          subtitle="5xx, last 5 min"
          icon={ServerCrash}
          status={now ? TONE_STATUS[shareTone(now.failed, now.total, 0.001, 0.05)] : "unknown"}
        />
        <StatCard
          title="Tenants served"
          value={report ? fmt(tenantCounts.served) : unmeasured}
          subtitle={
            report
              ? `${tenantCounts.quiet} gone quiet, ${tenantCounts.refused} refused (4xx), last 5 min`
              : "Named tenants, last 5 min"
          }
          icon={Users}
          status={
            report
              ? tenantCounts.quiet > 0
                ? "critical"
                : tenantCounts.refused > 0
                  ? "warning"
                  : "healthy"
              : "unknown"
          }
        />
      </div>

      {report && (
        <>
          <ClientHealthChart
            series={report.series}
            releases={report.releases}
            range={report.range}
            stepSeconds={report.step_seconds}
            generatedAt={report.generated_at}
          />
          <TenantHealthTable tenants={tenants} />
        </>
      )}

      <div className="grid gap-6 lg:grid-cols-2">
        {report ? (
          <MachinesPanel machines={report.machines} />
        ) : (
          <Card data-testid="machines-panel">
            <CardHeader>
              <CardTitle className="text-base font-medium">Machines</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-sm text-muted-foreground">{unmeasured}</p>
            </CardContent>
          </Card>
        )}
        <CoreInternals state={summary} />
      </div>
    </div>
  );
}

function CoreInternals({ state }: { state: SummaryState }) {
  const value = (v: number | null, unit = "") => (v === null ? "Not measured" : `${fmt(v)}${unit}`);
  return (
    <Card data-testid="core-internals">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base font-medium">
          <Gauge className="h-5 w-5" />
          Core internals
        </CardTitle>
        <CardDescription>
          Read from Core&rsquo;s own /metrics through the Query Service.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {state.kind === "loading" ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : state.kind === "error" ? (
          <p role="alert" className="text-sm text-destructive">
            {state.message}
          </p>
        ) : (
          <dl className="grid grid-cols-2 gap-4 text-sm">
            <div>
              <dt className="text-muted-foreground">Events stored</dt>
              <dd className="font-medium tabular-nums" data-testid="core-events-total">
                {value(state.summary.events_total)}
              </dd>
            </div>
            <div>
              <dt className="text-muted-foreground">Query latency p99</dt>
              <dd className="font-medium tabular-nums" data-testid="core-query-p99">
                {value(
                  state.summary.query_latency_p99_ms === null
                    ? null
                    : Math.round(state.summary.query_latency_p99_ms * 10) / 10,
                  " ms"
                )}
              </dd>
            </div>
          </dl>
        )}
      </CardContent>
    </Card>
  );
}
