"use client";

import { Badge, Button, Card, CardContent, Select, Textarea } from "@allsource/ui";
import {
  AlertTriangle,
  ArrowUpRight,
  CalendarClock,
  Check,
  ClipboardCheck,
  Inbox,
  Loader2,
  Mail,
  RefreshCw,
  UserRoundSearch,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import {
  DesignPartnerApiError,
  type DesignPartnerApplication,
  type DesignPartnerStatus,
  fetchDesignPartnerApplications,
  updateDesignPartnerStatus,
} from "@/lib/design-partners-api";

const statuses: DesignPartnerStatus[] = ["new", "reviewing", "accepted", "waitlisted", "rejected"];

const statusStyles: Record<DesignPartnerStatus, string> = {
  new: "border-blue-500/25 bg-blue-500/10 text-blue-700 dark:text-blue-300",
  reviewing: "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-300",
  accepted: "border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
  waitlisted: "border-violet-500/25 bg-violet-500/10 text-violet-700 dark:text-violet-300",
  rejected: "border-slate-500/25 bg-slate-500/10 text-slate-600 dark:text-slate-300",
};

const timelineLabels: Record<string, string> = {
  ready_now: "Ready now",
  within_30_days: "Within 30 days",
  within_60_days: "Within 60 days",
  exploring: "Exploring",
};

const shortDateFormatter = new Intl.DateTimeFormat("en", { month: "short", day: "numeric" });
const dateTimeFormatter = new Intl.DateTimeFormat("en", {
  dateStyle: "medium",
  timeStyle: "short",
});

export default function DesignPartnersPage() {
  const [applications, setApplications] = useState<DesignPartnerApplication[]>([]);
  const [selectedID, setSelectedID] = useState("");
  const [filter, setFilter] = useState<DesignPartnerStatus | "all">("all");
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    setLoadError("");
    try {
      const items = await fetchDesignPartnerApplications();
      setApplications(items);
      setSelectedID((current) =>
        current && items.some((item) => item.id === current) ? current : items[0]?.id || ""
      );
    } catch (error) {
      if (error instanceof DesignPartnerApiError && [401, 403].includes(error.status)) {
        setLoadError("Admin session is not authorized. Sign in again.");
      } else {
        setLoadError(error instanceof Error ? error.message : "Applications could not be loaded.");
      }
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const visible =
    filter === "all" ? applications : applications.filter((item) => item.status === filter);
  const selected = applications.find((item) => item.id === selectedID) || null;

  function replaceApplication(next: DesignPartnerApplication) {
    setApplications((current) => current.map((item) => (item.id === next.id ? next : item)));
  }

  const counts = Object.fromEntries(
    statuses.map((status) => [status, applications.filter((item) => item.status === status).length])
  ) as Record<DesignPartnerStatus, number>;

  return (
    <div className="space-y-6" data-testid="design-partners-page">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <p className="font-mono text-xs uppercase tracking-[0.16em] text-primary">Acquisition</p>
          <h1 className="mt-2 flex items-center gap-3 text-3xl font-bold tracking-tight">
            <UserRoundSearch className="h-7 w-7" aria-hidden="true" />
            Design partners
          </h1>
          <p className="mt-1 text-muted-foreground">
            Private application review and append-only decision history.
          </p>
        </div>
        <Button variant="outline" onClick={() => void load()} disabled={loading}>
          <RefreshCw className={`mr-2 h-4 w-4 ${loading ? "animate-spin" : ""}`} /> Refresh
        </Button>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <Metric label="Applications" value={applications.length} icon={Inbox} />
        <Metric label="New" value={counts.new} icon={Mail} />
        <Metric label="Reviewing" value={counts.reviewing} icon={ClipboardCheck} />
        <Metric label="Accepted" value={counts.accepted} icon={Check} />
      </div>

      {loadError && (
        <Card className="border-destructive/30 bg-destructive/5">
          <CardContent className="flex items-start gap-3 py-5" role="alert">
            <AlertTriangle className="mt-0.5 h-5 w-5 text-destructive" />
            <div>
              <p className="font-medium">Couldn&apos;t load applications</p>
              <p className="mt-1 text-sm text-muted-foreground">{loadError}</p>
            </div>
          </CardContent>
        </Card>
      )}

      <nav className="flex gap-2 overflow-x-auto pb-1" aria-label="Application status filters">
        <FilterButton active={filter === "all"} onClick={() => setFilter("all")}>
          All <span>{applications.length}</span>
        </FilterButton>
        {statuses.map((status) => (
          <FilterButton key={status} active={filter === status} onClick={() => setFilter(status)}>
            <span className="capitalize">{status}</span> <span>{counts[status]}</span>
          </FilterButton>
        ))}
      </nav>

      <div className="grid min-h-[36rem] gap-5 xl:grid-cols-[minmax(20rem,0.72fr)_minmax(0,1.28fr)]">
        <Card className="overflow-hidden">
          <div className="border-b px-5 py-4">
            <h2 className="font-semibold">Queue</h2>
            <p className="text-xs text-muted-foreground">Newest first · {visible.length} shown</p>
          </div>
          <div className="max-h-[46rem] overflow-y-auto">
            {loading && applications.length === 0 ? (
              <div className="flex items-center justify-center gap-2 p-12 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" /> Loading applications…
              </div>
            ) : visible.length === 0 ? (
              <div className="p-12 text-center text-sm text-muted-foreground">
                No applications in this view.
              </div>
            ) : (
              visible.map((application) => (
                <button
                  key={application.id}
                  type="button"
                  onClick={() => setSelectedID(application.id)}
                  className={`w-full border-b p-5 text-left transition-colors last:border-b-0 hover:bg-muted/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring ${selectedID === application.id ? "bg-primary/5" : ""}`}
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0">
                      <p className="truncate font-medium">{application.name}</p>
                      <p className="mt-1 truncate text-sm text-muted-foreground">
                        {application.project}
                      </p>
                    </div>
                    <StatusBadge status={application.status} />
                  </div>
                  <div className="mt-3 flex items-center justify-between gap-3 font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
                    <span>{application.campaign_source.source || "direct"}</span>
                    <span>{formatDate(application.submitted_at)}</span>
                  </div>
                </button>
              ))
            )}
          </div>
        </Card>

        {selected ? (
          <ApplicationDetail
            key={`${selected.id}:${selected.status_history.length}`}
            application={selected}
            onUpdated={replaceApplication}
          />
        ) : (
          <Card className="flex items-center justify-center p-12 text-center text-muted-foreground">
            Select an application to review its answers.
          </Card>
        )}
      </div>
    </div>
  );
}

function ApplicationDetail({
  application,
  onUpdated,
}: {
  application: DesignPartnerApplication;
  onUpdated: (application: DesignPartnerApplication) => void;
}) {
  const [status, setStatus] = useState<DesignPartnerStatus>(application.status);
  const [note, setNote] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  async function saveStatus() {
    setSaving(true);
    setError("");
    try {
      const updated = await updateDesignPartnerStatus(application.id, status, note);
      onUpdated(updated);
      setNote("");
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "Status could not be saved.");
    } finally {
      setSaving(false);
    }
  }

  const source = application.campaign_source;
  const usesCombinedAnswer =
    application.project === "Not provided" &&
    application.agent_use_case === application.memory_problem;
  return (
    <Card className="overflow-hidden">
      <div className="border-b p-5 sm:p-6">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-2xl font-semibold tracking-tight">{application.name}</h2>
              <StatusBadge status={application.status} />
            </div>
            {!usesCombinedAnswer && (
              <p className="mt-1 text-muted-foreground">{application.project}</p>
            )}
            <a
              href={`mailto:${application.email}`}
              className="mt-2 inline-flex items-center gap-2 text-sm text-primary underline underline-offset-4"
            >
              {application.email} <ArrowUpRight className="h-3.5 w-3.5" />
            </a>
          </div>
          <div className="font-mono text-[10px] uppercase tracking-wider text-muted-foreground sm:text-right">
            <p>{formatDateTime(application.submitted_at)}</p>
            <p className="mt-1">ID {application.id.slice(0, 10)}</p>
          </div>
        </div>
      </div>

      <CardContent className="space-y-7 p-5 sm:p-6">
        <div className="grid gap-5 sm:grid-cols-2">
          <Answer
            label={usesCombinedAnswer ? "Timing" : "Integration timeline"}
            body={
              usesCombinedAnswer
                ? "Discuss on fit call"
                : timelineLabels[application.timeline] || application.timeline
            }
          />
          <Answer
            label="Acquisition source"
            body={[source.source || "direct", source.medium, source.content]
              .filter(Boolean)
              .join(" · ")}
          />
        </div>
        {usesCombinedAnswer ? (
          <Answer label="Project and memory problem" body={application.memory_problem} />
        ) : (
          <>
            <Answer label="Agent use case" body={application.agent_use_case} />
            <Answer label="Current memory problem" body={application.memory_problem} />
          </>
        )}

        <div className="rounded-xl border bg-muted/30 p-4">
          <p className="font-mono text-[10px] uppercase tracking-[0.15em] text-muted-foreground">
            Campaign attribution
          </p>
          <dl className="mt-3 grid gap-x-6 gap-y-2 text-sm sm:grid-cols-2">
            <SourceRow label="Source" value={source.source || "direct"} />
            <SourceRow label="Medium" value={source.medium || "—"} />
            <SourceRow label="Campaign" value={source.campaign || "—"} />
            <SourceRow label="Content" value={source.content || "—"} />
          </dl>
        </div>

        <div className="border-t pt-6">
          <h3 className="font-semibold">Decision</h3>
          <div className="mt-4 grid gap-3 sm:grid-cols-[12rem_1fr_auto]">
            <Select
              value={status}
              onChange={(event) => setStatus(event.target.value as DesignPartnerStatus)}
              disabled={saving}
              aria-label="Application status"
            >
              {statuses.map((option) => (
                <option key={option} value={option}>
                  {option}
                </option>
              ))}
            </Select>
            <Textarea
              value={note}
              onChange={(event) => setNote(event.target.value)}
              maxLength={500}
              rows={2}
              disabled={saving}
              placeholder="Optional internal decision note"
              aria-label="Decision note"
            />
            <Button
              onClick={() => void saveStatus()}
              disabled={saving || status === application.status}
            >
              {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : "Save"}
            </Button>
          </div>
          {error && (
            <p className="mt-3 text-sm text-destructive" role="alert">
              {error}
            </p>
          )}
          {application.retention_until && (
            <p className="mt-3 flex items-center gap-2 text-xs text-muted-foreground">
              <CalendarClock className="h-3.5 w-3.5" /> Retention deadline{" "}
              {formatDateTime(application.retention_until)}
            </p>
          )}
        </div>

        <div className="border-t pt-6">
          <h3 className="font-semibold">Status history</h3>
          <ol className="relative mt-4 space-y-4 border-l pl-5">
            {application.status_history.map((item) => (
              <li key={`${item.status}-${item.changed_at}`}>
                <span className="absolute -left-1.5 mt-1.5 h-3 w-3 rounded-full border-2 border-background bg-primary" />
                <div className="flex flex-wrap items-center gap-2">
                  <StatusBadge status={item.status} />
                  <span className="text-xs text-muted-foreground">
                    {formatDateTime(item.changed_at)}
                  </span>
                </div>
                {item.note && <p className="mt-1 text-sm text-muted-foreground">{item.note}</p>}
                {item.actor && (
                  <p className="mt-1 font-mono text-[10px] text-muted-foreground">
                    by {item.actor}
                  </p>
                )}
              </li>
            ))}
          </ol>
        </div>
      </CardContent>
    </Card>
  );
}

function Metric({
  label,
  value,
  icon: Icon,
}: {
  label: string;
  value: number;
  icon: typeof Inbox;
}) {
  return (
    <Card>
      <CardContent className="flex items-center justify-between p-5">
        <div>
          <p className="text-sm text-muted-foreground">{label}</p>
          <p className="mt-1 text-3xl font-semibold">{value}</p>
        </div>
        <Icon className="h-5 w-5 text-primary" />
      </CardContent>
    </Card>
  );
}

function FilterButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex shrink-0 items-center gap-2 rounded-full border px-4 py-2 text-sm font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${active ? "border-primary bg-primary text-primary-foreground" : "bg-background hover:bg-muted"}`}
    >
      {children}
    </button>
  );
}

function StatusBadge({ status }: { status: DesignPartnerStatus }) {
  return (
    <Badge variant="outline" className={`capitalize ${statusStyles[status]}`}>
      {status}
    </Badge>
  );
}

function Answer({ label, body }: { label: string; body: string }) {
  return (
    <section>
      <p className="font-mono text-[10px] uppercase tracking-[0.15em] text-muted-foreground">
        {label}
      </p>
      <p className="mt-2 whitespace-pre-wrap text-sm leading-6">{body}</p>
    </section>
  );
}

function SourceRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-4">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="font-mono text-xs">{value}</dd>
    </div>
  );
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "—" : shortDateFormatter.format(date);
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "—" : dateTimeFormatter.format(date);
}
