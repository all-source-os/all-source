"use client";

import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  type ChartConfig,
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  Skeleton,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@allsource/ui";
import {
  Activity,
  ArrowLeft,
  Calendar,
  CreditCard,
  ExternalLink,
  KeyRound,
  ShieldAlert,
  Users,
  Zap,
} from "lucide-react";
import Link from "next/link";
import { useParams, useRouter } from "next/navigation";
import { useCallback, useEffect, useState } from "react";
import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { HealthChip } from "@/components/fleet/health-chip";
import { ClientKeysPanel } from "@/components/tenants/client-keys-panel";
import { CommsPanel } from "@/components/tenants/comms-panel";
import { OperationsPanel } from "@/components/tenants/operations-panel";
import { TenantHealthPanel } from "@/components/tenants/tenant-health-panel";
import { UsageBar } from "@/components/tenants/usage-bar";
import { ViewAsButton } from "@/components/tenants/view-as-button";
import {
  diagnoseIdentity,
  fetchTenantHealth,
  type HealthTier,
  type IdentityDiagnosis,
  type TenantHealth,
} from "@/lib/fleet-api";
import {
  fetchTenantDetail,
  fetchTenantInvoices,
  fetchTenantUsage,
  planLabel,
  suspendTenant,
  type TenantDetail,
  type TenantInvoice,
  type TenantStatus,
  type TenantUsage,
  unsuspendTenant,
  updateTenantQuotas,
} from "@/lib/tenants-api";

function statusVariant(status: TenantStatus): "default" | "secondary" | "destructive" | "outline" {
  switch (status) {
    case "active":
      return "default";
    case "suspended":
      return "destructive";
    case "archived":
      return "secondary";
    default:
      return "outline";
  }
}

// Date guards (§6): never call new Date()/toLocaleString on a possibly-undefined
// API field. A missing/invalid date renders an em-dash, never NaN/Invalid Date.
function formatDate(iso?: string): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric" });
}

function formatDateTime(iso?: string): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export default function TenantDetailPage() {
  const params = useParams();
  const router = useRouter();
  const tenantId = params.id as string;

  const [tenant, setTenant] = useState<TenantDetail | null>(null);
  const [usage, setUsage] = useState<TenantUsage | null>(null);
  const [health, setHealth] = useState<TenantHealth | null>(null);
  const [healthUnavailable, setHealthUnavailable] = useState(false);
  const [invoices, setInvoices] = useState<TenantInvoice[]>([]);
  const [identity, setIdentity] = useState<IdentityDiagnosis | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const loadData = useCallback(async () => {
    setIsLoading(true);
    // The core detail + usage are required; the auxiliary 360 reads (health,
    // invoices, identity diagnosis) are best-effort and must NOT block or fail
    // the page if their endpoint is unreachable — each settles independently.
    const [detailRes, usageRes, healthRes, invoicesRes, identityRes] = await Promise.allSettled([
      fetchTenantDetail(tenantId),
      fetchTenantUsage(tenantId),
      fetchTenantHealth(tenantId),
      fetchTenantInvoices(tenantId),
      diagnoseIdentity(tenantId),
    ]);

    setTenant(detailRes.status === "fulfilled" ? detailRes.value : null);
    setUsage(usageRes.status === "fulfilled" ? usageRes.value : null);

    if (healthRes.status === "fulfilled") {
      setHealth(healthRes.value);
      setHealthUnavailable(false);
    } else {
      console.error("Tenant health unavailable:", healthRes.reason);
      setHealth(null);
      setHealthUnavailable(true);
    }

    setInvoices(invoicesRes.status === "fulfilled" ? invoicesRes.value : []);
    setIdentity(identityRes.status === "fulfilled" ? identityRes.value : null);

    setIsLoading(false);
  }, [tenantId]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleSaveQuotas = async (quotas: {
    event_limit: number;
    query_limit: number;
    storage_limit_mb: number;
  }) => {
    await updateTenantQuotas(tenantId, quotas);
    await loadData();
  };

  const handleToggleSuspend = async () => {
    if (!tenant) return;
    if (tenant.status === "suspended") {
      await unsuspendTenant(tenantId);
    } else {
      await suspendTenant(tenantId);
    }
    await loadData();
  };

  const chartConfig: ChartConfig = {
    events: { label: "Events", color: "hsl(var(--primary))" },
  };

  if (isLoading) {
    return (
      <div className="space-y-6" data-testid="tenant-detail-loading">
        <Skeleton className="h-8 w-48" />
        <div className="grid gap-4 md:grid-cols-2">
          <Skeleton className="h-48" />
          <Skeleton className="h-48" />
        </div>
        <Skeleton className="h-64" />
      </div>
    );
  }

  if (!tenant) {
    return (
      <div className="space-y-4" data-testid="tenant-detail-error">
        <p className="text-muted-foreground">Tenant not found.</p>
        <Button variant="outline" onClick={() => router.push("/tenants")}>
          Back to Tenants
        </Button>
      </div>
    );
  }

  // Guard every list (§6) — the fetchers already array-guard, but re-assert here
  // so a stale state shape can never crash a .map.
  const members = Array.isArray(tenant.members) ? tenant.members : [];
  const apiKeys = Array.isArray(tenant.api_keys) ? tenant.api_keys : [];
  const auditLog = Array.isArray(tenant.audit_log) ? tenant.audit_log : [];
  const daily = Array.isArray(usage?.daily) ? usage.daily : [];

  const chartData = daily.map((d) => ({
    date: formatDate(d.date),
    events: d.events ?? 0,
  }));

  // Build the health signal → tier map for health-driven operation prominence.
  const signalTiers: Record<string, HealthTier> = {};
  for (const s of Array.isArray(health?.signals) ? health.signals : []) {
    signalTiers[s.signal] = s.tier;
  }

  // Empty-read / divergence symptoms (Pillar A — Recent errors section): pull
  // the relevant health signals + the read-only identity diagnosis.
  const symptomSignals = (Array.isArray(health?.signals) ? health.signals : []).filter((s) =>
    ["empty_read_symptom_rate", "last_event_age", "last_sync_age", "durability"].includes(s.signal)
  );

  return (
    <div className="space-y-6" data-testid="tenant-detail-page">
      {/* Breadcrumb */}
      <nav
        className="flex items-center gap-2 text-sm text-muted-foreground"
        data-testid="tenant-breadcrumb"
      >
        <Link
          href="/tenants"
          className="hover:text-foreground transition-colors flex items-center gap-1"
        >
          <ArrowLeft className="h-4 w-4" />
          Tenants
        </Link>
        <span>/</span>
        <span className="text-foreground font-medium">{tenant.name}</span>
      </nav>

      {/* Header */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-3">
          <div>
            <h1 className="text-3xl font-bold tracking-tight">{tenant.name}</h1>
            {tenant.description && (
              <p className="text-muted-foreground mt-1">{tenant.description}</p>
            )}
          </div>
          {health && <HealthChip tier={health.tier} />}
        </div>
        <div className="flex items-center gap-2">
          {/* Guarded read-only "View as tenant" (§5.3) — mints the scoped token
              server-side and drops into the read-only product frame. */}
          <ViewAsButton tenantId={tenant.id} />
          <Link href={`/fleet/${tenant.id}`}>
            <Button variant="outline" data-testid="tenant-link-fleet">
              <ExternalLink className="mr-2 h-4 w-4" />
              Fleet health
            </Button>
          </Link>
        </div>
      </div>

      {/* Identity + Subscription/billing */}
      <div className="grid gap-4 md:grid-cols-2">
        <Card data-testid="tenant-info-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Users className="h-5 w-5" />
              Identity
            </CardTitle>
            <CardDescription>From GET /api/v1/admin/tenants/{tenant.id}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex justify-between">
              <span className="text-sm text-muted-foreground">ID</span>
              <span className="text-sm font-mono">{tenant.id}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-sm text-muted-foreground">Status</span>
              <Badge variant={statusVariant(tenant.status)} data-testid="tenant-status-badge">
                {tenant.status}
              </Badge>
            </div>
            <div className="flex justify-between">
              <span className="text-sm text-muted-foreground">Plan</span>
              <Badge variant="outline" className="capitalize">
                {planLabel(tenant.plan)}
              </Badge>
            </div>
            <div className="flex justify-between">
              <span className="text-sm text-muted-foreground">Members</span>
              <span className="text-sm" data-testid="identity-member-count">
                {tenant.member_count ?? members.length ?? 0}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-sm text-muted-foreground">Events</span>
              <span className="text-sm tabular-nums" data-testid="identity-event-count">
                {(tenant.event_count ?? 0).toLocaleString()}
              </span>
            </div>
            {tenant.home_region && (
              <div className="flex justify-between">
                <span className="text-sm text-muted-foreground">Home region</span>
                <span className="text-sm">{tenant.home_region}</span>
              </div>
            )}
            <div className="flex justify-between">
              <span className="text-sm text-muted-foreground">Created</span>
              <span className="text-sm">{formatDate(tenant.created_at)}</span>
            </div>
          </CardContent>
        </Card>

        <Card data-testid="tenant-subscription-card">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <CreditCard className="h-5 w-5" />
              Subscription &amp; billing
            </CardTitle>
            <CardDescription>Detail subscription + admin billing invoices</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {tenant.subscription ? (
              <>
                <div className="flex justify-between">
                  <span className="text-sm text-muted-foreground">Plan</span>
                  <span className="text-sm font-medium capitalize">
                    {planLabel(tenant.subscription.plan)}
                  </span>
                </div>
                {tenant.subscription.status && (
                  <div className="flex justify-between">
                    <span className="text-sm text-muted-foreground">Status</span>
                    <Badge
                      variant={tenant.subscription.status === "active" ? "default" : "destructive"}
                    >
                      {tenant.subscription.status}
                    </Badge>
                  </div>
                )}
                <div className="flex justify-between">
                  <span className="text-sm text-muted-foreground">Started</span>
                  <span className="text-sm">{formatDate(tenant.subscription.started_at)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-sm text-muted-foreground">Current period end</span>
                  <span className="text-sm">
                    {formatDate(
                      tenant.subscription.renews_at ?? tenant.subscription.current_period_end
                    )}
                  </span>
                </div>
                {tenant.subscription.dunning_state && (
                  <div className="flex justify-between">
                    <span className="text-sm text-muted-foreground">Dunning</span>
                    <Badge variant="secondary">{tenant.subscription.dunning_state}</Badge>
                  </div>
                )}
                {tenant.subscription.grandfathered && (
                  <div className="flex justify-between">
                    <span className="text-sm text-muted-foreground">Grandfathered</span>
                    <Badge variant="outline">
                      until {formatDate(tenant.subscription.grandfather_until)}
                    </Badge>
                  </div>
                )}
              </>
            ) : (
              <p className="text-sm text-muted-foreground">
                No active subscription (free / self-host tenant).
              </p>
            )}

            {/* Recent invoices (admin billing, filtered to this tenant) */}
            <div className="pt-2" data-testid="tenant-invoices">
              <p className="text-xs font-medium text-muted-foreground mb-2">Recent invoices</p>
              {invoices.length === 0 ? (
                <p className="text-xs text-muted-foreground">No invoices on record.</p>
              ) : (
                <ul className="space-y-1.5">
                  {invoices.slice(0, 5).map((inv) => (
                    <li
                      key={inv.id}
                      className="flex items-center justify-between gap-2 text-xs"
                      data-testid={`invoice-row-${inv.id}`}
                    >
                      <span className="text-muted-foreground">{formatDate(inv.created_at)}</span>
                      <Badge
                        variant={
                          inv.status === "paid"
                            ? "default"
                            : inv.status === "failed"
                              ? "destructive"
                              : "secondary"
                        }
                      >
                        {inv.status}
                      </Badge>
                      <span className="font-mono tabular-nums">
                        {((inv.amount ?? 0) / 100).toLocaleString(undefined, {
                          style: "currency",
                          currency: inv.currency || "USD",
                        })}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Operations panel (Pillar B) — health-driven prominence */}
      <OperationsPanel
        tenantId={tenant.id}
        tenantName={tenant.name}
        status={tenant.status}
        quotas={tenant.quotas}
        signalTiers={signalTiers}
        onSaveQuotas={handleSaveQuotas}
        onToggleSuspend={handleToggleSuspend}
        onAfterRecovery={loadData}
      />

      {/* Communicate panel (Pillar C) — single-tenant notice / email / notes */}
      <CommsPanel tenantId={tenant.id} tenantName={tenant.name} />

      {/* Health panel (Pillar A — Health section) */}
      <TenantHealthPanel health={health} tenantId={tenant.id} unavailable={healthUnavailable} />

      {/* Usage trends (real Core counters via admin usage endpoint) */}
      <Card data-testid="tenant-usage-card">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Zap className="h-5 w-5" />
            Usage trends
          </CardTitle>
          <CardDescription>Current billing-period usage and quotas (Core metering)</CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          {!usage ? (
            <p
              className="py-4 text-center text-sm text-muted-foreground"
              data-testid="usage-unavailable"
            >
              Usage metering unavailable.
            </p>
          ) : (
            <>
              <div className="grid gap-4 sm:grid-cols-3">
                <div className="rounded-lg border p-4 space-y-1" data-testid="stat-events-ingested">
                  <p className="text-sm text-muted-foreground">Events Ingested</p>
                  <p className="text-2xl font-bold tabular-nums">
                    {(usage.events_ingested ?? 0).toLocaleString()}
                  </p>
                </div>
                <div className="rounded-lg border p-4 space-y-1" data-testid="stat-queries-run">
                  <p className="text-sm text-muted-foreground">Queries Run</p>
                  <p className="text-2xl font-bold tabular-nums">
                    {(usage.queries_run ?? 0).toLocaleString()}
                  </p>
                </div>
                <div className="rounded-lg border p-4 space-y-1" data-testid="stat-storage-used">
                  <p className="text-sm text-muted-foreground">Storage Used</p>
                  <p className="text-2xl font-bold tabular-nums">
                    {(usage.storage_used_mb ?? 0).toLocaleString()} MB
                  </p>
                </div>
              </div>

              <div className="space-y-4">
                <UsageBar
                  label="Events"
                  current={usage.events_ingested}
                  limit={usage.event_limit}
                  testId="usage-bar-events"
                />
                <UsageBar
                  label="Queries"
                  current={usage.queries_run}
                  limit={usage.query_limit}
                  testId="usage-bar-queries"
                />
                <UsageBar
                  label="Storage"
                  current={usage.storage_used_mb}
                  limit={usage.storage_limit_mb}
                  unit=" MB"
                  testId="usage-bar-storage"
                />
              </div>

              {chartData.length > 0 && (
                <div data-testid="daily-events-chart">
                  <p className="text-sm font-medium mb-2">Daily Events (Last 30 Days)</p>
                  <ChartContainer config={chartConfig} className="h-[200px] w-full">
                    <AreaChart data={chartData}>
                      <defs>
                        <linearGradient id="fill-events" x1="0" y1="0" x2="0" y2="1">
                          <stop offset="5%" stopColor="hsl(var(--primary))" stopOpacity={0.3} />
                          <stop offset="95%" stopColor="hsl(var(--primary))" stopOpacity={0} />
                        </linearGradient>
                      </defs>
                      <CartesianGrid strokeDasharray="3 3" vertical={false} />
                      <XAxis
                        dataKey="date"
                        tickLine={false}
                        axisLine={false}
                        tickMargin={8}
                        fontSize={12}
                      />
                      <YAxis tickLine={false} axisLine={false} tickMargin={8} fontSize={12} />
                      <ChartTooltip
                        content={
                          <ChartTooltipContent
                            formatter={(value) => (
                              <span className="font-mono font-medium">
                                {typeof value === "number" ? value.toLocaleString() : String(value)}
                              </span>
                            )}
                          />
                        }
                      />
                      <Area
                        type="monotone"
                        dataKey="events"
                        stroke="hsl(var(--primary))"
                        fill="url(#fill-events)"
                        strokeWidth={2}
                      />
                    </AreaChart>
                  </ChartContainer>
                </div>
              )}
            </>
          )}
        </CardContent>
      </Card>

      {/* Members */}
      <Card data-testid="tenant-members-card">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Users className="h-5 w-5" />
            Members
          </CardTitle>
          <CardDescription>
            {members.length} member{members.length !== 1 ? "s" : ""}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {members.length === 0 ? (
            <p className="text-sm text-muted-foreground py-4 text-center">No members found.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Email</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead>Joined</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {members.map((member) => (
                  <TableRow key={member.id} data-testid={`member-row-${member.id}`}>
                    <TableCell className="font-medium">{member.email}</TableCell>
                    <TableCell>
                      <Badge variant="outline">{member.role}</Badge>
                    </TableCell>
                    <TableCell>{formatDate(member.joined_at)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* API keys */}
      <Card data-testid="tenant-apikeys-card">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <KeyRound className="h-5 w-5" />
            API keys
          </CardTitle>
          <CardDescription>
            Key names + roles. The canonical role is{" "}
            <span className="font-mono">serviceaccount</span> — a drifted{" "}
            <span className="font-mono">service_account</span> silently 403s.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {apiKeys.length === 0 ? (
            <p
              className="text-sm text-muted-foreground py-4 text-center"
              data-testid="apikeys-empty"
            >
              No API keys surfaced for this tenant. Use Rotate keys in Operations to re-mint with
              the canonical role.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead>Created</TableHead>
                  <TableHead>Last used</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {apiKeys.map((key) => {
                  const roleOk = key.role === "serviceaccount";
                  return (
                    <TableRow key={key.id} data-testid={`apikey-row-${key.id}`}>
                      <TableCell className="font-medium">{key.name || key.id}</TableCell>
                      <TableCell>
                        <Badge variant={roleOk ? "outline" : "destructive"}>
                          {key.role || "unknown"}
                        </Badge>
                      </TableCell>
                      <TableCell>{formatDate(key.created_at)}</TableCell>
                      <TableCell>{formatDate(key.last_used_at)}</TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      <ClientKeysPanel tenantId={tenant.id} tenantName={tenant.name} />

      {/* Recent errors / empty-read symptoms */}
      <Card data-testid="tenant-symptoms-card">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <ShieldAlert className="h-5 w-5" />
            Recent errors &amp; empty-read symptoms
          </CardTitle>
          <CardDescription>
            Read-path / identity-divergence symptoms — from the health signals + the read-only
            identity diagnosis (no mutation).
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Identity divergence (the "data appears empty" root cause) */}
          <div
            className={
              identity?.divergence_detected
                ? "flex items-start gap-2 rounded-lg border border-red-500/30 bg-red-500/10 p-3 text-sm"
                : "flex items-start gap-2 rounded-lg border p-3 text-sm"
            }
            data-testid="symptom-identity"
          >
            <Activity className="h-4 w-4 shrink-0 mt-0.5 text-muted-foreground" />
            <div className="space-y-1">
              <p className="font-medium">
                Identity divergence:{" "}
                {identity ? (identity.divergence_detected ? "DETECTED" : "none") : "unavailable"}
              </p>
              {identity?.divergence_detected && (
                <p className="text-xs text-muted-foreground">
                  JWT tenant <span className="font-mono">{identity.jwt_tenant_id || "—"}</span> vs
                  store <span className="font-mono">{identity.store_tenant_id || "—"}</span>.{" "}
                  {identity.remediation || "Re-login resolves a JWT/store mismatch."}
                </p>
              )}
            </div>
          </div>

          {/* Symptom-relevant health signals */}
          {symptomSignals.length === 0 ? (
            <p className="text-sm text-muted-foreground" data-testid="symptoms-none">
              No empty-read / staleness symptoms reported.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Signal</TableHead>
                  <TableHead>Value</TableHead>
                  <TableHead>Tier</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {symptomSignals.map((s) => (
                  <TableRow key={s.signal} data-testid={`symptom-signal-${s.signal}`}>
                    <TableCell className="font-mono text-sm">{s.signal}</TableCell>
                    <TableCell className="font-medium">{s.value}</TableCell>
                    <TableCell>
                      <HealthChip tier={s.tier} />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>

      {/* Audit timeline */}
      <Card data-testid="tenant-audit-card">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Calendar className="h-5 w-5" />
            Audit timeline
          </CardTitle>
          <CardDescription>
            Tenant audit + recovery events (admin.recovery.*) targeting this tenant.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {auditLog.length === 0 ? (
            <p className="text-sm text-muted-foreground py-4 text-center">No audit entries.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Action</TableHead>
                  <TableHead>Actor</TableHead>
                  <TableHead>Details</TableHead>
                  <TableHead>Timestamp</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {auditLog.map((entry) => (
                  <TableRow key={entry.id} data-testid={`audit-row-${entry.id}`}>
                    <TableCell className="font-medium">{entry.action}</TableCell>
                    <TableCell>{entry.actor}</TableCell>
                    <TableCell className="text-muted-foreground">{entry.details || "-"}</TableCell>
                    <TableCell>{formatDateTime(entry.timestamp)}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
