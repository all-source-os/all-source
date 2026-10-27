"use client";

import {
  Badge,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import { ANONYMOUS_TENANT, isAlarming, orderTenants } from "@/lib/client-health";
import type { TenantHealth, TenantState } from "@/lib/metrics-api";

const STATE_BADGE: Record<
  TenantState,
  { label: string; variant: "default" | "secondary" | "destructive" | "outline" }
> = {
  served: { label: "Served", variant: "default" },
  quiet: { label: "Gone quiet", variant: "destructive" },
  refused: { label: "Refused", variant: "destructive" },
  idle: { label: "Idle", variant: "outline" },
};

const fmt = (n: number) => n.toLocaleString("en-US");
const fmtRate = (n: number) => n.toLocaleString("en-US", { maximumFractionDigits: 1 });

function TenantName({ tenant }: { tenant: string }) {
  return (
    <>
      <span className="font-mono text-xs wrap-anywhere">{tenant}</span>
      {tenant === ANONYMOUS_TENANT && (
        <span className="block break-words text-xs text-muted-foreground">
          Requests whose authentication failed
        </span>
      )}
    </>
  );
}

export function TenantHealthTable({ tenants }: { tenants: TenantHealth[] }) {
  const rows = orderTenants(tenants);
  return (
    <Card data-testid="tenant-health">
      <CardHeader>
        <CardTitle className="text-base font-medium">Tenants</CardTitle>
        <CardDescription>
          Client data requests per tenant in the last 5 minutes. &ldquo;Gone quiet&rdquo; means the
          tenant averaged at least 10 requests/min over the previous 24 hours and almost nothing was
          served in the last 5 minutes. Refused counts every 4xx, including 429.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {rows.length === 0 ? (
          <p className="text-sm text-muted-foreground" data-testid="tenant-health-empty">
            No tenant sent client data requests in the last 24 hours.
          </p>
        ) : (
          <>
            <ul className="space-y-3 sm:hidden" data-testid="tenant-cards">
              {rows.map((t) => {
                const badge = STATE_BADGE[t.state] ?? STATE_BADGE.idle;
                return (
                  <li
                    key={t.tenant}
                    data-testid={`tenant-card-${t.tenant}`}
                    data-state={t.state}
                    className={cn(
                      "space-y-2 rounded-lg border p-3",
                      isAlarming(t.state) && "bg-red-500/5"
                    )}
                  >
                    <div className="flex items-start justify-between gap-3">
                      <div className="min-w-0">
                        <TenantName tenant={t.tenant} />
                      </div>
                      <Badge variant={badge.variant} className="shrink-0">
                        {badge.label}
                      </Badge>
                    </div>
                    <dl className="grid grid-cols-2 gap-x-4 gap-y-1 text-xs">
                      <dt className="text-muted-foreground">Served 5m</dt>
                      <dd className="text-right tabular-nums">{fmt(t.served_5m)}</dd>
                      <dt className="text-muted-foreground">Refused 5m</dt>
                      <dd className="text-right tabular-nums">{fmt(t.refused_5m)}</dd>
                      <dt className="text-muted-foreground">Failed 5m</dt>
                      <dd className="text-right tabular-nums">{fmt(t.failed_5m)}</dd>
                      <dt className="text-muted-foreground">24h avg /min</dt>
                      <dd className="text-right tabular-nums">{fmtRate(t.baseline_per_min)}</dd>
                    </dl>
                  </li>
                );
              })}
            </ul>
            <div className="hidden sm:block">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Tenant</TableHead>
                    <TableHead>State</TableHead>
                    <TableHead className="text-right">Served 5m</TableHead>
                    <TableHead className="text-right">Refused 5m</TableHead>
                    <TableHead className="text-right">Failed 5m</TableHead>
                    <TableHead className="text-right">24h avg /min</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((t) => {
                    const badge = STATE_BADGE[t.state] ?? STATE_BADGE.idle;
                    return (
                      <TableRow
                        key={t.tenant}
                        data-testid={`tenant-row-${t.tenant}`}
                        data-state={t.state}
                        className={cn(isAlarming(t.state) && "bg-red-500/5")}
                      >
                        <TableCell className="max-w-[18rem] whitespace-normal">
                          <TenantName tenant={t.tenant} />
                        </TableCell>
                        <TableCell>
                          <Badge variant={badge.variant}>{badge.label}</Badge>
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {fmt(t.served_5m)}
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {fmt(t.refused_5m)}
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {fmt(t.failed_5m)}
                        </TableCell>
                        <TableCell className="text-right tabular-nums">
                          {fmtRate(t.baseline_per_min)}
                        </TableCell>
                      </TableRow>
                    );
                  })}
                </TableBody>
              </Table>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
