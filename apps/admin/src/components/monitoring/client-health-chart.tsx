"use client";

import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  type ChartConfig,
  ChartContainer,
  ChartLegend,
  ChartLegendContent,
  ChartTooltip,
  ChartTooltipContent,
} from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import { Area, AreaChart, CartesianGrid, ReferenceLine, XAxis, YAxis } from "recharts";
import { groupReleases, shortApp } from "@/lib/client-health";
import type { ClientHealthPoint, Release, TimeRange } from "@/lib/metrics-api";

// No success/warning tokens exist. Tailwind emits a --color-* palette variable only
// when a utility uses it; the monitoring page uses green-600 and yellow-500.
const chartConfig = {
  served: { label: "Served (2xx)", color: "var(--color-green-600)" },
  refused_auth: { label: "Refused by auth (401/403)", color: "var(--destructive)" },
  throttled: { label: "Throttled (429)", color: "var(--color-yellow-500)" },
  failed: { label: "Failed (5xx)", color: "var(--foreground)" },
} satisfies ChartConfig;

const SERIES = ["served", "refused_auth", "throttled", "failed"] as const;

const RANGES: { value: TimeRange; label: string }[] = [
  { value: "1h", label: "1H" },
  { value: "24h", label: "24H" },
  { value: "7d", label: "7D" },
];

const RANGE_SPAN_MS: Record<TimeRange, number> = {
  "1h": 3_600_000,
  "24h": 86_400_000,
  "7d": 604_800_000,
};

const STEP_MS: Record<TimeRange, number> = { "1h": 60_000, "24h": 300_000, "7d": 3_600_000 };

export function RangeSelector({
  range,
  onRangeChange,
}: {
  range: TimeRange;
  onRangeChange: (range: TimeRange) => void;
}) {
  return (
    <fieldset className="m-0 flex gap-1 border-0 p-0" data-testid="range-selector">
      <legend className="sr-only">Time range</legend>
      {RANGES.map((r) => (
        <button
          type="button"
          key={r.value}
          onClick={() => onRangeChange(r.value)}
          aria-pressed={range === r.value}
          className={cn(
            "min-h-9 min-w-9 rounded-md px-2.5 py-1 text-xs font-medium transition-colors",
            range === r.value
              ? "bg-primary text-primary-foreground"
              : "text-muted-foreground hover:bg-muted hover:text-foreground"
          )}
        >
          {r.label}
        </button>
      ))}
    </fieldset>
  );
}

interface ClientHealthChartProps {
  series: ClientHealthPoint[];
  releases: Release[];
  /** The range the series was fetched for, not the one being requested. */
  range: TimeRange;
  stepSeconds?: number;
  generatedAt?: string;
}

function formatTick(ms: number, range: TimeRange): string {
  const d = new Date(ms);
  return range === "7d"
    ? d.toLocaleDateString([], { month: "short", day: "numeric" })
    : d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function formatMarker(ms: number, range: TimeRange): string {
  const d = new Date(ms);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return range === "7d"
    ? `${d.toLocaleDateString([], { month: "short", day: "numeric" })} ${time}`
    : time;
}

export function ClientHealthChart({
  series,
  releases,
  range,
  stepSeconds,
  generatedAt,
}: ClientHealthChartProps) {
  const data = series.map((p) => ({ ...p, ts: Date.parse(p.t) }));
  const generated = generatedAt ? Date.parse(generatedAt) : Number.NaN;
  const last = data[data.length - 1]?.ts ?? (Number.isFinite(generated) ? generated : Date.now());
  const rangeStart = last - RANGE_SPAN_MS[range];
  const stepMs = stepSeconds ? stepSeconds * 1000 : STEP_MS[range];
  const inRange = releases.filter((r) => {
    const ts = Date.parse(r.started_at);
    return ts >= rangeStart && ts <= last + stepMs;
  });
  const groups = groupReleases(inRange, stepMs);
  const first = data[0]?.ts ?? rangeStart;
  const domainMin = Math.min(first, groups[0]?.ts ?? first);
  const domainMax = Math.max(last, groups[groups.length - 1]?.ts ?? last);
  const stepLabel = stepSeconds ? `${Math.round(stepSeconds / 60)} min` : "step";

  return (
    <Card data-testid="chart-client-requests">
      <CardHeader className="space-y-1 pb-2">
        <CardTitle className="text-base font-medium">Client data requests</CardTitle>
        <CardDescription>
          Requests per {stepLabel} to events, event feed, read models and stats. Dashed lines are
          machine starts (deploys and restarts).
        </CardDescription>
      </CardHeader>
      <CardContent>
        {data.length === 0 ? (
          <div
            className="flex h-[280px] items-center justify-center text-sm text-muted-foreground"
            data-testid="chart-client-requests-empty"
          >
            Prometheus returned no client requests for this range.
          </div>
        ) : (
          <ChartContainer config={chartConfig} className="h-[280px] w-full">
            <AreaChart data={data}>
              <CartesianGrid strokeDasharray="3 3" vertical={false} />
              <XAxis
                dataKey="ts"
                type="number"
                scale="time"
                domain={[domainMin, domainMax]}
                tickFormatter={(v: number) => formatTick(v, range)}
                tickLine={false}
                axisLine={false}
                tickMargin={8}
                fontSize={12}
              />
              <YAxis
                tickLine={false}
                axisLine={false}
                tickMargin={8}
                fontSize={12}
                allowDecimals={false}
              />
              <ChartTooltip
                content={
                  <ChartTooltipContent
                    labelFormatter={(_, payload) => {
                      const ts = (payload?.[0] as { payload?: { ts?: unknown } } | undefined)
                        ?.payload?.ts;
                      return typeof ts === "number" ? new Date(ts).toLocaleString() : "";
                    }}
                  />
                }
              />
              <ChartLegend content={<ChartLegendContent />} />
              {SERIES.map((key) => (
                <Area
                  key={key}
                  type="monotone"
                  dataKey={key}
                  stackId="requests"
                  stroke={`var(--color-${key})`}
                  fill={`var(--color-${key})`}
                  fillOpacity={0.35}
                  strokeWidth={1.5}
                  isAnimationActive={false}
                />
              ))}
              {groups.map((g) => (
                <ReferenceLine
                  key={g.ts}
                  x={g.ts}
                  stroke="var(--muted-foreground)"
                  strokeDasharray="4 4"
                  label={{
                    value: `${g.label} ${formatMarker(g.ts, range)}`,
                    position: "insideTopLeft",
                    fontSize: 10,
                    fill: "var(--muted-foreground)",
                  }}
                />
              ))}
            </AreaChart>
          </ChartContainer>
        )}
        {inRange.length > 0 && (
          <ul
            className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground"
            data-testid="release-markers"
          >
            {inRange.map((r) => (
              <li key={`${r.app}-${r.instance}-${r.started_at}`} className="break-words">
                {shortApp(r.app)} {r.instance} started {new Date(r.started_at).toLocaleString()}
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
