import type { NextRequest } from "next/server";
import {
  historyEnabled,
  historyError,
  historyForm,
  historyLogin,
  historyQuery,
  historyRequestAllowed,
  historySession,
  validHistoryId,
} from "@/lib/server/event-history-http";
import {
  escapeHtml,
  historyPage,
  historyUnavailable,
  signInAgain,
} from "@/lib/server/event-history-view";

type Context = { params: Promise<{ workspace: string; event: string }> };

function evidencePath(workspace: string, event: string) {
  return (
    "/event-history/workspaces/" +
    encodeURIComponent(workspace) +
    "/events/" +
    encodeURIComponent(event)
  );
}

function refusal(status: number, path: string) {
  if (status === 401) return signInAgain();
  if (status === 403 || status === 404)
    return historyPage(
      "Event unavailable",
      "<p>Use an account with event-read access to the workspace that issued this citation.</p>",
      404
    );
  if (status === 409)
    return historyPage(
      "This request was already received",
      `<p>Opening the event again starts a new read.</p><a href="${escapeHtml(path)}">Start a new read</a>`,
      409
    );
  if (status === 402 || status === 429)
    return historyPage(
      "Workspace query limit reached",
      "<p>Your workspace owner can check the query allowance in AllSource.</p>",
      status
    );
  return historyUnavailable(path);
}

export async function GET(request: NextRequest, context: Context) {
  if (!historyEnabled()) return historyError(404);
  const { workspace, event } = await context.params;
  if (!validHistoryId(workspace) || !validHistoryId(event)) return historyError(404);
  const path = evidencePath(workspace, event);
  const session = historySession(request);
  if (!session) return request.cookies.has("auth_token") ? signInAgain() : historyLogin(path);
  try {
    const result = await historyQuery(
      "evidence-intent",
      { workspace_id: workspace, event_id: event },
      "Bearer " + session
    );
    if (!result.ok) return refusal(result.status, path);
    const intent = result.data.intent;
    if (typeof intent !== "string" || !intent || intent.length > 3800)
      return historyUnavailable(path);
    return historyPage(
      "Open cited event",
      `<p>Read the original workspace record behind this citation. Opening it uses one workspace query unit.</p>
<form method="post"><input type="hidden" name="intent" value="${escapeHtml(intent)}"><button>Open event</button></form>`
    );
  } catch {
    return historyUnavailable(path);
  }
}

const releaseVersion = (value: unknown): value is string =>
  typeof value === "string" && /^[A-Za-z0-9._-]{1,48}$/.test(value);

export async function POST(request: NextRequest, context: Context) {
  if (!historyEnabled()) return historyError(404);
  if (!historyRequestAllowed(request)) return historyError(403);
  const { workspace, event } = await context.params;
  if (!validHistoryId(workspace) || !validHistoryId(event)) return historyError(404);
  const path = evidencePath(workspace, event);
  try {
    const form = await historyForm(request);
    if (form?.get("decision") === "signin") return historyLogin(path, true);
    const session = historySession(request);
    if (!session) return signInAgain();
    const intent = form?.get("intent");
    if (!intent || intent.length > 3800 || form?.size !== 1) return historyError(400);
    const result = await historyQuery(
      "evidence",
      { workspace_id: workspace, event_id: event, intent },
      "Bearer " + session
    );
    if (!result.ok) return refusal(result.status, path);
    const row = result.data.event;
    const space = result.data.workspace;
    if (
      space?.id !== workspace ||
      typeof space.name !== "string" ||
      row?.id !== event ||
      typeof row.entity !== "string" ||
      typeof row.timestamp !== "string" ||
      !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d{1,9})?Z$/.test(row.timestamp) ||
      !Number.isFinite(Date.parse(row.timestamp))
    )
      return historyUnavailable(path);
    let change = "Event recorded";
    if (row.kind === "released" && releaseVersion(row.version)) change = "Released " + row.version;
    if (
      row.kind === "rolled_back" &&
      releaseVersion(row.from_version) &&
      releaseVersion(row.to_version)
    )
      change = "Rolled back from " + row.from_version + " to " + row.to_version;
    return historyPage(
      change,
      `<dl><dt>Workspace</dt><dd>${escapeHtml(space.name)}</dd><dt>Entity</dt><dd>${escapeHtml(row.entity)}</dd>
<dt>Recorded (UTC)</dt><dd><time>${escapeHtml(row.timestamp)}</time></dd><dt>Event ID</dt><dd>${escapeHtml(row.id)}</dd></dl>`
    );
  } catch {
    return historyUnavailable(path);
  }
}
