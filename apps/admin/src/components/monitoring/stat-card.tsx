"use client";

import { Card, CardContent } from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import type { LucideIcon } from "lucide-react";

interface StatCardProps {
  title: string;
  value: string;
  subtitle?: string;
  icon: LucideIcon;
  trend?: "up" | "down" | "neutral";
  status?: "healthy" | "warning" | "critical" | "unknown";
}

export function StatCard({
  title,
  value,
  subtitle,
  icon: Icon,
  status = "healthy",
}: StatCardProps) {
  return (
    <Card data-testid={`stat-card-${title.toLowerCase().replace(/[\s/]+/g, "-")}`}>
      <CardContent className="p-6">
        <div className="flex items-center justify-between">
          <div className="space-y-1">
            <p className="text-sm font-medium text-muted-foreground">{title}</p>
            <p className="text-2xl font-bold tabular-nums" data-testid="stat-value">
              {value}
            </p>
            {subtitle && <p className="text-xs text-muted-foreground">{subtitle}</p>}
          </div>
          <div
            className={cn(
              "flex h-12 w-12 items-center justify-center rounded-lg",
              status === "healthy" && "bg-green-500/10 text-green-500",
              status === "warning" && "bg-yellow-500/10 text-yellow-500",
              status === "critical" && "bg-red-500/10 text-red-500",
              status === "unknown" && "bg-muted text-muted-foreground"
            )}
          >
            <Icon className="h-6 w-6" />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
