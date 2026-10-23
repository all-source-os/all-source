"use client";

import { Badge, Button, Card, CardContent, CardHeader, CardTitle } from "@allsource/ui";
import { ChevronLeft, ChevronRight, FileText, Filter, Shield } from "lucide-react";
import { useState } from "react";
import { LoadError } from "@/components/dashboard/load-error";
import { FadeIn } from "@/components/ui/fade-in";
import { useAuditLogs } from "@/hooks/use-audit-logs";

const PAGE_SIZE = 25;

const ACTION_LABELS: Record<string, string> = {
  "api_key.created": "API Key Created",
  "api_key.revoked": "API Key Revoked",
  "member.invited": "Member Invited",
  "member.removed": "Member Removed",
  "plan.changed": "Plan Changed",
  "webhook.created": "Webhook Created",
  "webhook.deleted": "Webhook Deleted",
};

const ACTION_COLORS: Record<string, string> = {
  "api_key.created": "bg-blue-500/10 text-blue-500",
  "api_key.revoked": "bg-red-500/10 text-red-500",
  "member.invited": "bg-green-500/10 text-green-500",
  "member.removed": "bg-orange-500/10 text-orange-500",
  "plan.changed": "bg-purple-500/10 text-purple-500",
  "webhook.created": "bg-cyan-500/10 text-cyan-500",
  "webhook.deleted": "bg-red-500/10 text-red-500",
};

function formatTimestamp(ts: string): string {
  try {
    return new Date(ts).toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return ts;
  }
}

function formatDetails(details: Record<string, unknown>): string {
  const parts: string[] = [];
  for (const [key, value] of Object.entries(details)) {
    if (value !== null && value !== undefined && value !== "") {
      parts.push(`${key}: ${String(value)}`);
    }
  }
  return parts.join(", ");
}

export default function AuditLogPage() {
  const [actionFilter, setActionFilter] = useState<string | undefined>(undefined);
  const [page, setPage] = useState(0);

  const { entries, retentionDays, actions, isLoading, error, refresh } = useAuditLogs({
    action: actionFilter,
    limit: PAGE_SIZE,
    offset: page * PAGE_SIZE,
  });

  const hasNextPage = entries.length === PAGE_SIZE;
  const hasPrevPage = page > 0;

  return (
    <div className="space-y-6">
      {/* Header */}
      <FadeIn delay={0.1} inView>
        <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h1 className="text-2xl font-bold tracking-tight md:text-3xl">Audit Log</h1>
            <p className="mt-1 text-muted-foreground">
              Track actions taken on your workspace
              {retentionDays !== null && retentionDays > 0 ? ` (last ${retentionDays} days)` : ""}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <Shield className="h-4 w-4 text-muted-foreground" />
            <span className="text-sm text-muted-foreground">
              {retentionDays === -1
                ? "Available audit history"
                : retentionDays !== null && retentionDays > 0
                  ? `${retentionDays}-day retention`
                  : "Audit history"}
            </span>
          </div>
        </div>
      </FadeIn>

      {/* Action filter */}
      <FadeIn delay={0.2} inView>
        <Card>
          <CardContent className="flex flex-wrap items-center gap-2 p-4">
            <Filter className="h-4 w-4 shrink-0 text-muted-foreground" />
            <span className="text-sm font-medium">Filter:</span>
            <button
              type="button"
              onClick={() => {
                setActionFilter(undefined);
                setPage(0);
              }}
              className={`rounded-full border px-3 py-1 text-xs transition-colors ${
                actionFilter === undefined
                  ? "border-primary bg-primary/10 text-primary"
                  : "border-border text-muted-foreground hover:border-primary/50"
              }`}
            >
              All
            </button>
            {actions.map((action) => (
              <button
                type="button"
                key={action}
                onClick={() => {
                  setActionFilter(action === actionFilter ? undefined : action);
                  setPage(0);
                }}
                className={`rounded-full border px-3 py-1 text-xs transition-colors ${
                  actionFilter === action
                    ? "border-primary bg-primary/10 text-primary"
                    : "border-border text-muted-foreground hover:border-primary/50"
                }`}
              >
                {ACTION_LABELS[action] || action}
              </button>
            ))}
          </CardContent>
        </Card>
      </FadeIn>

      {/* Audit log entries */}
      <FadeIn delay={0.3} inView>
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <FileText className="h-5 w-5" />
              Activity
            </CardTitle>
          </CardHeader>
          <CardContent>
            {isLoading ? (
              <div className="flex items-center justify-center py-12">
                <div className="h-6 w-6 animate-spin rounded-full border-2 border-primary border-t-transparent" />
              </div>
            ) : error ? (
              <LoadError
                title="Audit activity could not be loaded"
                message={error}
                onRetry={refresh}
              />
            ) : entries.length === 0 ? (
              <div className="py-12 text-center">
                <Shield className="mx-auto mb-4 h-12 w-12 text-muted-foreground/50" />
                <p className="text-sm text-muted-foreground">
                  {actionFilter
                    ? "No audit log entries for this action type."
                    : "No audit log entries yet. Actions will appear here as they occur."}
                </p>
              </div>
            ) : (
              <div className="space-y-1">
                {/* Table header */}
                <div className="hidden grid-cols-[180px_1fr_1fr_1.5fr] gap-4 border-b border-border px-4 py-2 text-xs font-medium uppercase tracking-wider text-muted-foreground sm:grid">
                  <span>Timestamp</span>
                  <span>Actor</span>
                  <span>Action</span>
                  <span>Details</span>
                </div>

                {entries.map((entry) => (
                  <div
                    key={entry.id}
                    className="grid grid-cols-1 gap-1 rounded-lg px-4 py-3 text-sm transition-colors hover:bg-muted/50 sm:grid-cols-[180px_1fr_1fr_1.5fr] sm:items-center sm:gap-4"
                  >
                    <span className="text-xs text-muted-foreground sm:text-sm">
                      {formatTimestamp(entry.timestamp)}
                    </span>
                    <span className="truncate font-medium">{entry.actor}</span>
                    <span>
                      <Badge variant="secondary" className={ACTION_COLORS[entry.action] || ""}>
                        {ACTION_LABELS[entry.action] || entry.action}
                      </Badge>
                    </span>
                    <span className="truncate text-muted-foreground">
                      {formatDetails(entry.details)}
                    </span>
                  </div>
                ))}
              </div>
            )}

            {/* Pagination */}
            {(hasPrevPage || hasNextPage) && (
              <div className="mt-4 flex items-center justify-between border-t border-border pt-4">
                <Button
                  variant="outline"
                  size="sm"
                  disabled={!hasPrevPage}
                  onClick={() => setPage((p) => Math.max(0, p - 1))}
                >
                  <ChevronLeft className="mr-1 h-4 w-4" />
                  Previous
                </Button>
                <span className="text-sm text-muted-foreground">Page {page + 1}</span>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={!hasNextPage}
                  onClick={() => setPage((p) => p + 1)}
                >
                  Next
                  <ChevronRight className="ml-1 h-4 w-4" />
                </Button>
              </div>
            )}
          </CardContent>
        </Card>
      </FadeIn>
    </div>
  );
}
