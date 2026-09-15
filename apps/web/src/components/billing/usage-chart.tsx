"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@allsource/ui";
import { cn } from "@allsource/ui/utils";

interface UsageChartProps {
  title: string;
  used: number;
  quota: number;
  unit?: string;
  history?: number[];
  /** Honest message shown when there is no time-series data (never "demo"). */
  emptyLabel?: string;
}

export function UsageChart({
  title,
  used,
  quota,
  unit = "",
  history = [],
  emptyLabel = "No data for this period",
}: UsageChartProps) {
  const unlimited = quota < 0;
  const percentage = quota > 0 ? (used / quota) * 100 : 0;
  const isWarning = percentage >= 80;
  const isCritical = percentage >= 95;

  const chartData = history;
  const hasChartData = chartData.length > 0;
  const maxValue = hasChartData ? Math.max(...chartData, quota) : quota;

  const formatNumber = (num: number) => {
    if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
    if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
    return num.toString();
  };

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-base font-medium">{title}</CardTitle>
      </CardHeader>
      <CardContent>
        {/* Current usage */}
        <div className="mb-4 flex items-baseline justify-between">
          <div>
            <span className="text-3xl font-bold tabular-nums">{formatNumber(used)}</span>
            <span className="ml-1 text-muted-foreground">
              {unlimited ? unit : `/ ${formatNumber(quota)} ${unit}`}
            </span>
          </div>
          <span
            className={cn(
              "text-sm font-medium",
              isCritical
                ? "text-destructive"
                : isWarning
                  ? "text-yellow-600"
                  : "text-muted-foreground"
            )}
          >
            {unlimited ? "Unlimited" : `${percentage.toFixed(1)}%`}
          </span>
        </div>

        {/* Progress bar */}
        <div className="mb-4 h-3 overflow-hidden rounded-full bg-muted">
          <div
            className={cn(
              "h-full rounded-full transition-all duration-500",
              isCritical ? "bg-destructive" : isWarning ? "bg-yellow-500" : "bg-primary"
            )}
            style={{ width: `${Math.min(percentage, 100)}%` }}
          />
        </div>

        {/* Area chart */}
        <div className="h-24 w-full">
          {!hasChartData ? (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              {emptyLabel}
            </div>
          ) : (
            <svg
              viewBox={`0 0 ${chartData.length * 4} 100`}
              className="h-full w-full"
              preserveAspectRatio="none"
            >
              {/* Gradient definitions */}
              <defs>
                <linearGradient id={`usageGradient-${title}`} x1="0" y1="0" x2="0" y2="1">
                  <stop
                    offset="0%"
                    stopColor={
                      isCritical
                        ? "rgb(239, 68, 68)"
                        : isWarning
                          ? "rgb(234, 179, 8)"
                          : "hsl(var(--primary))"
                    }
                    stopOpacity="0.3"
                  />
                  <stop
                    offset="100%"
                    stopColor={
                      isCritical
                        ? "rgb(239, 68, 68)"
                        : isWarning
                          ? "rgb(234, 179, 8)"
                          : "hsl(var(--primary))"
                    }
                    stopOpacity="0"
                  />
                </linearGradient>
              </defs>

              {/* Quota line */}
              <line
                x1="0"
                y1={100 - (quota / maxValue) * 90}
                x2={chartData.length * 4}
                y2={100 - (quota / maxValue) * 90}
                stroke="hsl(var(--muted-foreground))"
                strokeWidth="1"
                strokeDasharray="4,4"
                opacity="0.5"
              />

              {/* Area fill */}
              <path
                d={`
                M 0 100
                ${chartData
                  .map((value, i) => `L ${i * 4} ${100 - (value / maxValue) * 90}`)
                  .join(" ")}
                L ${(chartData.length - 1) * 4} 100
                Z
              `}
                fill={`url(#usageGradient-${title})`}
              />

              {/* Line */}
              <path
                d={`
                M 0 ${100 - ((chartData[0] ?? 0) / maxValue) * 90}
                ${chartData
                  .map((value, i) => `L ${i * 4} ${100 - (value / maxValue) * 90}`)
                  .join(" ")}
              `}
                fill="none"
                stroke={
                  isCritical
                    ? "rgb(239, 68, 68)"
                    : isWarning
                      ? "rgb(234, 179, 8)"
                      : "hsl(var(--primary))"
                }
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              />

              {/* Overage zone */}
              {percentage > 100 && (
                <rect
                  x="0"
                  y="0"
                  width={chartData.length * 4}
                  height={100 - (quota / maxValue) * 90}
                  fill="rgb(239, 68, 68)"
                  opacity="0.1"
                />
              )}
            </svg>
          )}
        </div>

        {/* Labels */}
        <div className="mt-2 flex justify-between text-xs text-muted-foreground">
          <span>30 days ago</span>
          <span>Today</span>
        </div>
      </CardContent>
    </Card>
  );
}
