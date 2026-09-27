"use client";

import { Button, Input, Select, Textarea } from "@allsource/ui";
import { ArrowUpRight, Copy, Plus, RefreshCw } from "lucide-react";
import { cloneElement, useCallback, useEffect, useId, useRef, useState } from "react";
import {
  emptyPartnership,
  fetchPartnershipHistory,
  fetchPartnerships,
  type PartnershipMessage,
  type PartnershipRecord,
  type PartnershipRevision,
  parsePartnershipImport,
  partnershipKinds,
  partnershipPriority,
  partnershipPrompt,
  partnershipStats,
  partnershipStatuses,
  safePartnershipURL,
  savePartnership,
} from "@/lib/partnerships-api";

function date(value: string): string {
  if (!value) return "Not recorded";
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? "Invalid date" : parsed.toLocaleString();
}
function LinkOut({ href, children }: { href: string; children: React.ReactNode }) {
  const safe = safePartnershipURL(href);
  return safe ? (
    <a
      className="inline-flex items-center gap-1 break-words text-primary underline underline-offset-4"
      href={safe}
      target="_blank"
      rel="noopener noreferrer"
    >
      {children}
      <ArrowUpRight className="h-4 w-4 shrink-0" aria-hidden="true" />
    </a>
  ) : (
    <span>{children}</span>
  );
}
function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactElement<{ id?: string }>;
}) {
  const id = useId();
  return (
    <div className="grid min-w-0 gap-2 text-sm font-medium">
      <label htmlFor={id}>{label}</label>
      {cloneElement(children, { id })}
    </div>
  );
}

export default function PartnershipsPage() {
  const [records, setRecords] = useState<PartnershipRevision[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState("");
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("");
  const [kind, setKind] = useState("");
  const [tab, setTab] = useState("research");
  const [edit, setEdit] = useState<PartnershipRecord | null>(null);
  const [editRevision, setEditRevision] = useState(0);
  const [messageOpen, setMessageOpen] = useState(false);
  const [imports, setImports] = useState<PartnershipRecord[]>([]);
  const [history, setHistory] = useState<PartnershipRevision[] | null>(null);
  const [guideOpen, setGuideOpen] = useState(false);
  const importRef = useRef<HTMLInputElement>(null);
  const historyRequest = useRef(0);
  const active = records.find((r) => r.record.id === selected);
  const stats = partnershipStats(records);
  const filtered = records
    .filter(
      ({ record: r }) =>
        (!status || r.status === status) &&
        (!kind || r.kind === kind) &&
        `${r.organization} ${r.geography} ${r.angle}`.toLowerCase().includes(query.toLowerCase())
    )
    .sort((a, b) => (partnershipPriority(b.record) ?? -1) - (partnershipPriority(a.record) ?? -1));

  const load = useCallback(async () => {
    setLoading(true);
    setLoadError("");
    try {
      setRecords(await fetchPartnerships());
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : "Could not load partnerships.");
    } finally {
      setLoading(false);
    }
  }, []);
  useEffect(() => {
    void load();
  }, [load]);

  function select(id: string) {
    historyRequest.current++;
    setSelected(id);
    setTab("research");
    setHistory(null);
    setError("");
    setNotice("");
    setMessageOpen(false);
  }
  async function save(record: PartnershipRecord, revision: number): Promise<boolean> {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await savePartnership(record, revision);
      setRecords((previous) => [
        ...previous.filter((r) => r.record.id !== result.record.id),
        result,
      ]);
      setSelected(result.record.id);
      setHistory(null);
      setNotice("Saved to Core. No external message was sent.");
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : "Save failed. Your draft is still here.");
      return false;
    } finally {
      setBusy(false);
    }
  }
  async function copyPrompt() {
    try {
      await navigator.clipboard.writeText(partnershipPrompt(active?.record.id));
      setNotice("Task prompt copied. Run it in Codex with this repository open.");
    } catch {
      setGuideOpen(true);
      setError("Clipboard unavailable. Select and copy the task prompt below.");
    }
  }
  async function importRecords() {
    setBusy(true);
    setError("");
    setNotice("");
    const failures: string[] = [];
    let created = 0;
    // Each record is a separate optimistic write. Never claim an atomic batch.
    for (const record of imports) {
      try {
        await savePartnership(record, 0);
        created++;
      } catch (e) {
        failures.push(
          `${record.organization}: ${e instanceof Error ? e.message : "Import failed"}`
        );
      }
    }
    setImports([]);
    if (importRef.current) importRef.current.value = "";
    setNotice(
      `Import finished: ${created} created, ${failures.length} not imported. Existing records were not overwritten.`
    );
    setError(failures.join("\n"));
    setBusy(false);
    await load();
  }

  return (
    <div className="min-w-0 space-y-6" data-testid="partnerships-page">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div className="max-w-2xl">
          <p className="mb-2 text-sm font-medium text-muted-foreground">
            Commercial exploration / founder workspace
          </p>
          <h1 className="text-3xl font-semibold tracking-tight">Partnerships</h1>
          <p className="mt-2 text-muted-foreground">
            Routes to real workloads. Research, conversations and pilots—not fundraising.
          </p>
        </div>
        <Button
          disabled={busy || !!edit || messageOpen || loading || !!loadError}
          onClick={() => {
            setEdit(emptyPartnership());
            setEditRevision(0);
            setMessageOpen(false);
            setError("");
          }}
        >
          <Plus className="mr-2 h-4 w-4" />
          Add organisation
        </Button>
      </header>

      <div className="flex flex-wrap items-center gap-3 border-y border-border py-3 text-sm">
        <span>
          <strong>{stats.researched}</strong> organisations
        </span>
        <span aria-hidden="true">/</span>
        <span>
          <strong>{stats.sent}</strong> contacted with proof
        </span>
        <span aria-hidden="true">/</span>
        <span>
          <strong>{stats.replies}</strong> organisations with recorded replies
        </span>
        <span className="text-muted-foreground">
          {stats.unchecked} contacted firms have no reply check recorded.
        </span>
      </div>
      {loadError && (
        <div role="alert" className="rounded-md border border-destructive p-4">
          <p>{loadError}</p>
          <Button className="mt-2" variant="outline" onClick={load}>
            Retry loading
          </Button>
        </div>
      )}
      {error && (
        <p
          role="alert"
          className="whitespace-pre-wrap break-words rounded-md border border-destructive p-4 text-sm"
        >
          {error}
        </p>
      )}
      {notice && (
        <p role="status" className="rounded-md border border-border bg-muted p-3 text-sm">
          {notice}
        </p>
      )}

      <div className="flex flex-wrap gap-2">
        <Button variant="outline" disabled={busy || !!edit} onClick={load}>
          <RefreshCw className="mr-2 h-4 w-4" />
          Refresh
        </Button>
        <Button variant="outline" onClick={copyPrompt}>
          <Copy className="mr-2 h-4 w-4" />
          Copy research task
        </Button>
        <Button variant="ghost" onClick={() => setGuideOpen(!guideOpen)} aria-expanded={guideOpen}>
          Research &amp; send workflow
        </Button>
      </div>
      {guideOpen && (
        <section
          className="space-y-4 border border-border bg-muted/40 p-4"
          aria-label="Research and sending workflow"
        >
          <h2 className="text-xl font-semibold">
            Evidence → review → approved send → verified record
          </h2>
          <p className="max-w-3xl text-sm">
            Use <code>$allsource-partnerships</code> from this repository. It checks existing
            history, researches public professional sources, ranks evidence with Jev when available
            and drafts an individual enquiry. You approve exact messages before sending through an
            authenticated channel. This dashboard does not send emails or social messages.
          </p>
          <Textarea
            aria-label="Research task prompt"
            readOnly
            value={partnershipPrompt(active?.record.id)}
            rows={6}
          />
          <p className="text-sm text-muted-foreground">
            Sent does not mean read, interested or paid. Existing approvals apply only to their
            original sends. Suppressed firms must not be contacted.
          </p>
          <details>
            <summary className="cursor-pointer font-medium">
              Import private research and send history
            </summary>
            <p className="my-3 text-sm">
              Choose a private JSON file with a <code>records</code> array (maximum 100
              organisations / 4 MB). Review names before importing. Create-only: matching website
              hostnames are rejected, not overwritten. Never commit real correspondence to the
              public repository.
            </p>
            <Input
              ref={importRef}
              type="file"
              accept="application/json,.json"
              aria-label="Partnership import file"
              disabled={busy || !!edit || loading || !!loadError}
              onChange={async (e) => {
                const file = e.target.files?.[0];
                setImports([]);
                setError("");
                if (!file) return;
                try {
                  if (file.size > 4_000_000) throw new Error("Import exceeds 4 MB.");
                  setImports(parsePartnershipImport(await file.text()));
                } catch (err) {
                  setError(err instanceof Error ? err.message : "Invalid import file.");
                }
              }}
            />
            {imports.length > 0 && (
              <div className="mt-3 space-y-3">
                <p className="break-words text-sm">
                  {imports.length} records: {imports.map((r) => r.organization).join(", ")}
                </p>
                <Button variant="outline" disabled={busy} onClick={importRecords}>
                  Import {imports.length} records (no sends)
                </Button>
              </div>
            )}
          </details>
        </section>
      )}

      {edit ? (
        <RecordEditor
          key={edit.id || "new"}
          record={edit}
          busy={busy}
          onCancel={() => {
            setEdit(null);
            setError("");
          }}
          onSave={async (r) => {
            if (await save(r, editRevision)) setEdit(null);
          }}
        />
      ) : (
        <>
          <div className="grid gap-3 sm:grid-cols-3">
            <Field label="Search organisations">
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Name, geography or angle"
              />
            </Field>
            <Field label="Pipeline">
              <Select value={status} onChange={(e) => setStatus(e.target.value)}>
                <option value="">All stages</option>
                {Object.entries(partnershipStatuses).map(([key, label]) => (
                  <option key={key} value={key}>
                    {label}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="Organisation type">
              <Select value={kind} onChange={(e) => setKind(e.target.value)}>
                <option value="">All types</option>
                {Object.entries(partnershipKinds).map(([key, label]) => (
                  <option key={key} value={key}>
                    {label}
                  </option>
                ))}
              </Select>
            </Field>
          </div>
          {loading ? (
            <p role="status">Loading partnerships…</p>
          ) : (
            !loadError && (
              <div className="grid min-w-0 gap-6 xl:grid-cols-[minmax(240px,0.8fr)_minmax(0,2fr)]">
                <section aria-label="Organisation register" className="min-w-0">
                  <p className="mb-3 text-sm text-muted-foreground">
                    {filtered.length} shown · highest evidence priority first
                  </p>
                  <ul className="divide-y divide-border border-y border-border">
                    {filtered.map(({ record: r }) => (
                      <li key={r.id}>
                        <button
                          type="button"
                          aria-pressed={selected === r.id}
                          disabled={busy || messageOpen}
                          className={`w-full p-4 text-left transition-colors focus-visible:outline-2 focus-visible:outline-ring ${selected === r.id ? "bg-accent text-accent-foreground" : "hover:bg-muted"}`}
                          onClick={() => select(r.id)}
                        >
                          <div className="flex justify-between gap-3">
                            <span className="font-semibold">{r.organization}</span>
                            <span className="shrink-0 text-sm tabular-nums">
                              <span className="sr-only">Evidence priority: </span>
                              {partnershipPriority(r)?.toFixed(1) ?? "—"}
                              <span className="text-muted-foreground"> / 9</span>
                            </span>
                          </div>
                          <p className="mt-1 text-sm">
                            {partnershipKinds[r.kind]} · {r.geography || "Geography unchecked"}
                          </p>
                          <p className="mt-2 text-sm text-muted-foreground">
                            {partnershipStatuses[r.status]}
                          </p>
                        </button>
                      </li>
                    ))}
                  </ul>
                  {filtered.length === 0 && (
                    <p className="py-8 text-muted-foreground">
                      {records.length
                        ? "No matching organisations. Clear filters to see the full register."
                        : "No records yet. Add an organisation or import the private research pack from Research & send workflow."}
                    </p>
                  )}
                  <p className="mt-4 text-sm text-muted-foreground">
                    Priority = fit + channel leverage + access. Not a win probability. Paid demand
                    is separate.
                  </p>
                </section>
                <section className="min-w-0" aria-label="Partnership detail">
                  {!active ? (
                    <div className="border border-dashed border-border p-8">
                      <h2 className="text-xl font-semibold">Start with the evidence</h2>
                      <p className="mt-2 max-w-lg text-muted-foreground">
                        Choose an organisation to inspect its route, research limits and exact
                        conversation history.
                      </p>
                    </div>
                  ) : (
                    <div className="space-y-5">
                      <div className="flex flex-wrap items-start justify-between gap-3">
                        <div>
                          <h2 className="text-2xl font-semibold">{active.record.organization}</h2>
                          <p className="mt-1 text-sm">
                            <LinkOut href={active.record.website}>{active.record.id}</LinkOut>
                          </p>
                        </div>
                        <Button
                          variant="outline"
                          disabled={busy || messageOpen}
                          onClick={() => {
                            setEdit(active.record);
                            setEditRevision(active.revision);
                          }}
                        >
                          Edit record
                        </Button>
                      </div>
                      {active.record.status === "do_not_contact" && (
                        <p className="border border-destructive p-3 font-medium">
                          Do not contact. Suppression is active.
                        </p>
                      )}
                      <div className="border-l-2 border-primary pl-4">
                        <p className="text-sm font-medium">
                          Next action
                          {active.record.next_action_at
                            ? ` · ${date(active.record.next_action_at)}`
                            : " · no date set"}
                        </p>
                        <p className="mt-1 whitespace-pre-wrap break-words">
                          {active.record.next_action || "No next action recorded."}
                        </p>
                      </div>
                      <nav
                        className="flex flex-wrap gap-2 border-b border-border pb-3"
                        aria-label="Detail sections"
                      >
                        {[
                          ["research", "Research"],
                          ["messages", `Messages (${active.record.messages.length})`],
                          ["history", "Record history"],
                        ].map(([value, label]) => (
                          <Button
                            key={value}
                            disabled={busy || messageOpen}
                            variant={tab === value ? "secondary" : "ghost"}
                            aria-pressed={tab === value}
                            onClick={async () => {
                              setTab(value ?? "research");
                              setError("");
                              const request = ++historyRequest.current;
                              if (value === "history") {
                                setHistory(null);
                                try {
                                  const revisions = await fetchPartnershipHistory(active.record.id);
                                  if (request === historyRequest.current) setHistory(revisions);
                                } catch (e) {
                                  if (request === historyRequest.current) {
                                    setHistory([]);
                                    setError(
                                      e instanceof Error ? e.message : "History unavailable"
                                    );
                                  }
                                }
                              }
                            }}
                          >
                            {label}
                          </Button>
                        ))}
                      </nav>
                      {tab === "research" && <Research record={active.record} />}
                      {tab === "messages" && (
                        <div className="space-y-5">
                          <p className="text-sm text-muted-foreground">
                            Last reply check: {date(active.record.reply_checked_at)}. No record does
                            not mean no reply.
                          </p>
                          <Button
                            variant="outline"
                            disabled={busy}
                            onClick={() => setMessageOpen(!messageOpen)}
                            aria-expanded={messageOpen}
                          >
                            {messageOpen ? "Close interaction form" : "Record draft or interaction"}
                          </Button>
                          {messageOpen && (
                            <MessageEditor
                              busy={busy}
                              onSave={async (message) => {
                                if (
                                  await save(
                                    {
                                      ...active.record,
                                      messages: [...active.record.messages, message],
                                    },
                                    active.revision
                                  )
                                )
                                  setMessageOpen(false);
                              }}
                            />
                          )}
                          {active.record.messages.length === 0 && (
                            <p className="text-muted-foreground">
                              No messages recorded. Research first, then review a specific draft.
                            </p>
                          )}
                          {[...active.record.messages].reverse().map((m) => (
                            <article key={m.id} className="space-y-3 border-t border-border pt-4">
                              <div className="flex flex-wrap justify-between gap-2 text-sm">
                                <p className="font-semibold capitalize">
                                  {m.direction} · {m.channel} · {m.outcome}
                                </p>
                                <time>{date(m.occurred_at)}</time>
                              </div>
                              <p className="break-all text-sm">To / from: {m.destination}</p>
                              {m.subject && <h3 className="font-semibold">{m.subject}</h3>}
                              <p className="max-w-prose whitespace-pre-wrap break-words leading-relaxed">
                                {m.body}
                              </p>
                              {m.verification && (
                                <p className="break-words text-sm text-muted-foreground">
                                  Verification: {m.verification}
                                </p>
                              )}
                              {m.approval_note && (
                                <p className="break-words text-sm text-muted-foreground">
                                  Historical approval: {m.approval_note}
                                </p>
                              )}
                            </article>
                          ))}
                        </div>
                      )}
                      {tab === "history" && (
                        <div className="space-y-4">
                          <p className="text-sm text-muted-foreground">
                            Core revisions preserve who changed this record and when.
                          </p>
                          {history === null ? (
                            <p>Loading history…</p>
                          ) : (
                            [...history].reverse().map((h) => (
                              <details key={h.revision} className="border-b border-border pb-3">
                                <summary className="cursor-pointer text-sm">
                                  Revision {h.revision} · {date(h.saved_at)} · {h.actor}
                                </summary>
                                <pre className="mt-3 max-h-96 overflow-auto rounded-md bg-muted p-3 text-xs">
                                  {JSON.stringify(h.record, null, 2)}
                                </pre>
                              </details>
                            ))
                          )}
                        </div>
                      )}
                      <p className="border-t border-border pt-4 text-sm text-muted-foreground">
                        Revision {active.revision} · saved {date(active.saved_at)}
                      </p>
                    </div>
                  )}
                </section>
              </div>
            )
          )}
        </>
      )}
    </div>
  );
}

function Research({ record: r }: { record: PartnershipRecord }) {
  return (
    <div className="space-y-6">
      <section>
        <h3 className="font-semibold">Commercial angle</h3>
        <p className="mt-2 max-w-prose whitespace-pre-wrap break-words leading-relaxed">
          {r.angle || "Not recorded."}
        </p>
      </section>
      <section>
        <h3 className="font-semibold">Contact route</h3>
        <p className="mt-2 whitespace-pre-wrap break-words">
          {r.contact_route || "No verified route recorded."}
        </p>
      </section>
      <section>
        <h3 className="font-semibold">Limits and unknowns</h3>
        <p className="mt-2 whitespace-pre-wrap break-words text-muted-foreground">
          {r.limitations || "No limitations documented. Review before outreach."}
        </p>
      </section>
      <section>
        <h3 className="font-semibold">Evidence score</h3>
        {r.score ? (
          <>
            <dl className="my-3 grid grid-cols-2 gap-3 text-sm">
              {[
                ["Product fit", r.score.fit],
                ["Channel leverage", r.score.leverage],
                ["Commercial access", r.score.access],
                ["Paid-demand evidence", r.score.paid_demand],
              ].map(([label, value]) => (
                <div key={label}>
                  <dt className="text-muted-foreground">{label}</dt>
                  <dd className="font-medium">{value} / 3</dd>
                </div>
              ))}
            </dl>
            <p className="text-sm">
              {r.score.model} · {date(r.score.run_at)}
            </p>
            <p className="mt-2 whitespace-pre-wrap text-sm text-muted-foreground">
              {r.score.rationale}
            </p>
          </>
        ) : (
          <p className="mt-2 text-muted-foreground">Unscored. No model result recorded.</p>
        )}
      </section>
      <section>
        <h3 className="font-semibold">Source evidence</h3>
        {r.sources.length === 0 && (
          <p className="mt-2 text-muted-foreground">No public sources recorded.</p>
        )}
        <ol className="mt-3 space-y-4">
          {r.sources.map((s) => (
            <li key={`${s.url}-${s.checked_at}-${s.title}`} className="border-l border-border pl-4">
              <LinkOut href={s.url}>{s.title}</LinkOut>
              <p className="mt-2 whitespace-pre-wrap break-words text-sm leading-relaxed">
                {s.evidence}
              </p>
              <p className="mt-1 text-sm text-muted-foreground">Checked {date(s.checked_at)}</p>
            </li>
          ))}
        </ol>
      </section>
      {r.notes && (
        <section>
          <h3 className="font-semibold">Operator notes</h3>
          <p className="mt-2 whitespace-pre-wrap break-words leading-relaxed">{r.notes}</p>
        </section>
      )}
    </div>
  );
}

function RecordEditor({
  record,
  busy,
  onCancel,
  onSave,
}: {
  record: PartnershipRecord;
  busy: boolean;
  onCancel: () => void;
  onSave: (r: PartnershipRecord) => Promise<void>;
}) {
  const [draft, setDraft] = useState(record);
  const [sources, setSources] = useState(JSON.stringify(record.sources, null, 2));
  const [score, setScore] = useState(JSON.stringify(record.score, null, 2));
  const [error, setError] = useState("");
  const update = (field: keyof PartnershipRecord, value: string) =>
    setDraft((prev) => ({ ...prev, [field]: value }));
  return (
    <form
      className="space-y-5 border border-border p-4 sm:p-6"
      onSubmit={async (e) => {
        e.preventDefault();
        setError("");
        try {
          const parsedSources = JSON.parse(sources);
          const parsedScore = JSON.parse(score);
          if (!Array.isArray(parsedSources)) throw new Error("Sources must be a JSON array.");
          await onSave({ ...draft, sources: parsedSources, score: parsedScore });
        } catch (err) {
          setError(err instanceof Error ? err.message : "Invalid JSON.");
        }
      }}
    >
      <h2 className="text-xl font-semibold">
        {record.id ? "Edit organisation" : "New organisation"}
      </h2>
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Organisation">
          <Input
            required
            maxLength={160}
            value={draft.organization}
            onChange={(e) => update("organization", e.target.value)}
          />
        </Field>
        <Field label="Website (identity)">
          <Input
            required
            type="url"
            readOnly={!!record.id}
            value={draft.website}
            onChange={(e) => update("website", e.target.value)}
            placeholder="https://example.com"
          />
        </Field>
        <Field label="Type">
          <Select value={draft.kind} onChange={(e) => update("kind", e.target.value)}>
            {Object.entries(partnershipKinds).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Geography / target office">
          <Input
            maxLength={250}
            value={draft.geography}
            onChange={(e) => update("geography", e.target.value)}
          />
        </Field>
        <Field label="Stage">
          <Select
            value={draft.status}
            disabled={record.status === "do_not_contact"}
            onChange={(e) => update("status", e.target.value)}
          >
            {Object.entries(partnershipStatuses).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Next action date">
          <Input
            type="date"
            value={draft.next_action_at.slice(0, 10)}
            onChange={(e) =>
              update("next_action_at", e.target.value ? `${e.target.value}T12:00:00Z` : "")
            }
          />
        </Field>
      </div>
      <Field label="Next action">
        <Input
          maxLength={2000}
          value={draft.next_action}
          onChange={(e) => update("next_action", e.target.value)}
        />
      </Field>
      <Field label="Commercial angle">
        <Textarea rows={3} value={draft.angle} onChange={(e) => update("angle", e.target.value)} />
      </Field>
      <Field label="Verified contact route">
        <Textarea
          rows={2}
          value={draft.contact_route}
          onChange={(e) => update("contact_route", e.target.value)}
        />
      </Field>
      <Field label="Limits and unknowns">
        <Textarea
          rows={3}
          value={draft.limitations}
          onChange={(e) => update("limitations", e.target.value)}
        />
      </Field>
      <Field label="Operator notes / corrections">
        <Textarea rows={3} value={draft.notes} onChange={(e) => update("notes", e.target.value)} />
      </Field>
      <Field label="Last reply check (UTC, only after checking)">
        <Input
          type="datetime-local"
          value={draft.reply_checked_at.slice(0, 16)}
          onChange={(e) =>
            update("reply_checked_at", e.target.value ? `${e.target.value}:00Z` : "")
          }
        />
      </Field>
      <details>
        <summary className="cursor-pointer font-medium">Research data: sources and score</summary>
        <div className="mt-4 space-y-4">
          <p className="text-sm text-muted-foreground">
            Each source needs url, title, evidence and checked_at. Score is null or model, run_at,
            fit, leverage, access, paid_demand (0–3), rationale. Import or use the skill to prepare
            structured research.
          </p>
          <Field label="Sources JSON">
            <Textarea rows={8} value={sources} onChange={(e) => setSources(e.target.value)} />
          </Field>
          <Field label="Score JSON">
            <Textarea rows={6} value={score} onChange={(e) => setScore(e.target.value)} />
          </Field>
        </div>
      </details>
      {error && <p role="alert">{error}</p>}
      {draft.status === "do_not_contact" && record.status !== "do_not_contact" && (
        <p className="text-sm font-medium">
          Saving Do not contact permanently suppresses this organisation in this workflow.
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <Button disabled={busy} type="submit">
          {busy ? "Saving…" : "Save record"}
        </Button>
        <Button disabled={busy} type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    </form>
  );
}

function MessageEditor({
  busy,
  onSave,
}: {
  busy: boolean;
  onSave: (m: PartnershipMessage) => Promise<void>;
}) {
  const [m, setM] = useState<PartnershipMessage>({
    id: crypto.randomUUID(),
    channel: "email",
    direction: "outbound",
    outcome: "draft",
    destination: "",
    subject: "",
    body: "",
    occurred_at: "",
    verification: "",
    approval_note: "",
  });
  const update = (field: keyof PartnershipMessage, value: string) =>
    setM((prev) => ({ ...prev, [field]: value }));
  return (
    <form
      className="space-y-4 border border-border bg-muted/30 p-4"
      onSubmit={async (e) => {
        e.preventDefault();
        await onSave(m);
      }}
    >
      <h3 className="font-semibold">Record only—does not send</h3>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Channel">
          <Select value={m.channel} onChange={(e) => update("channel", e.target.value)}>
            {["email", "linkedin", "x", "form", "other"].map((v) => (
              <option key={v}>{v}</option>
            ))}
          </Select>
        </Field>
        <Field label="Outcome">
          <Select
            value={m.outcome}
            onChange={(e) =>
              setM((prev) => ({
                ...prev,
                outcome: e.target.value as PartnershipMessage["outcome"],
                direction: e.target.value === "received" ? "inbound" : "outbound",
              }))
            }
          >
            {["draft", "sent", "received", "failed", "unknown"].map((v) => (
              <option key={v}>{v}</option>
            ))}
          </Select>
        </Field>
      </div>
      <Field label="Recipient / sender address or profile">
        <Input
          required
          value={m.destination}
          onChange={(e) => update("destination", e.target.value)}
        />
      </Field>
      <Field label="Subject (optional)">
        <Input value={m.subject} onChange={(e) => update("subject", e.target.value)} />
      </Field>
      <Field label="Exact message">
        <Textarea
          required
          rows={6}
          value={m.body}
          onChange={(e) => update("body", e.target.value)}
        />
      </Field>
      <Field label="Interaction timestamp (UTC)">
        <Input
          required={m.outcome !== "draft"}
          type="datetime-local"
          value={m.occurred_at.slice(0, 16)}
          onChange={(e) => update("occurred_at", e.target.value ? `${e.target.value}:00Z` : "")}
        />
      </Field>
      <Field label="Verification: provider ID / conversation URL and observed result">
        <Textarea
          required={["sent", "received"].includes(m.outcome)}
          rows={2}
          value={m.verification}
          onChange={(e) => update("verification", e.target.value)}
        />
      </Field>
      <Field label="Approval reference (historical; not future permission)">
        <Input value={m.approval_note} onChange={(e) => update("approval_note", e.target.value)} />
      </Field>
      <p className="text-sm text-muted-foreground">
        Recorded interactions are immutable. Add corrections in operator notes. If success could not
        be verified, choose unknown—not sent.
      </p>
      <Button disabled={busy} type="submit" variant="outline">
        Save interaction
      </Button>
    </form>
  );
}
