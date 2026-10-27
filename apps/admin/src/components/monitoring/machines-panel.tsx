"use client";

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@allsource/ui";
import { cn } from "@allsource/ui/utils";
import { Server } from "lucide-react";
import { formatUptime, type Machine } from "@/lib/metrics-api";

export function MachinesPanel({ machines }: { machines: Machine[] }) {
  return (
    <Card data-testid="machines-panel">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base font-medium">
          <Server className="h-5 w-5" />
          Machines
        </CardTitle>
        <CardDescription>
          Fly machines for the control plane, Core and the Query Service, as Fly reports them.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {machines.length === 0 ? (
          <p className="text-sm text-muted-foreground">Fly reported no machines for these apps.</p>
        ) : (
          <div className="space-y-3">
            {machines.map((m) => (
              <div
                key={`${m.app}-${m.instance}`}
                className="flex flex-col gap-3 rounded-lg border p-4 sm:flex-row sm:items-center sm:justify-between sm:gap-4"
                data-testid={`machine-${m.app}-${m.instance}`}
              >
                <div className="flex min-w-0 items-center gap-3">
                  <div
                    className={cn(
                      "h-3 w-3 shrink-0 rounded-full",
                      m.up ? "bg-green-500" : "bg-red-500"
                    )}
                  />
                  <div className="min-w-0">
                    <p className="break-words font-medium">{m.app}</p>
                    <p className="break-all font-mono text-xs text-muted-foreground">
                      {m.instance}
                    </p>
                  </div>
                </div>
                <div className="text-sm sm:text-right">
                  <p className={cn("font-medium", m.up ? "text-green-500" : "text-red-500")}>
                    {m.up ? "Up" : "Down"}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    Started {new Date(m.started_at).toLocaleString()}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    Uptime {formatUptime(m.uptime_seconds)}
                  </p>
                </div>
              </div>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
