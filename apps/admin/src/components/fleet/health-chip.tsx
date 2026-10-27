"use client";

import { Badge } from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import { type HealthTier, tierLabel } from "@/lib/fleet-api";

/**
 * Health chip — a coloured dot + tier label, using the dot+colour vocabulary
 * of the monitoring machines panel:
 *   green  → healthy
 *   yellow → degraded
 *   red    → at_risk / critical
 * At-Risk and Critical are both red dots but kept distinct by the label + badge
 * variant.
 */

interface HealthChipProps {
  tier: HealthTier;
  /** Render just the dot (table cells / dense rows) without the badge label. */
  dotOnly?: boolean;
  className?: string;
}

function dotColour(tier: HealthTier): string {
  switch (tier) {
    case "healthy":
      return "bg-green-500";
    case "degraded":
      return "bg-yellow-500";
    case "at_risk":
    case "critical":
      return "bg-red-500";
    default:
      return "bg-muted-foreground";
  }
}

function badgeVariant(tier: HealthTier): "default" | "secondary" | "destructive" | "outline" {
  switch (tier) {
    case "critical":
    case "at_risk":
      return "destructive";
    case "degraded":
      return "secondary";
    case "healthy":
      return "outline";
    default:
      return "outline";
  }
}

export function HealthChip({ tier, dotOnly, className }: HealthChipProps) {
  if (dotOnly) {
    return (
      <span
        role="img"
        className={cn("inline-block h-3 w-3 rounded-full", dotColour(tier), className)}
        data-testid={`health-dot-${tier}`}
        aria-label={tierLabel(tier)}
        title={tierLabel(tier)}
      />
    );
  }

  return (
    <Badge
      variant={badgeVariant(tier)}
      className={cn("inline-flex items-center gap-1.5", className)}
      data-testid={`health-chip-${tier}`}
    >
      <span className={cn("h-2 w-2 rounded-full", dotColour(tier))} />
      {tierLabel(tier)}
    </Badge>
  );
}
