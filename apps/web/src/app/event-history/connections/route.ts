import type { NextRequest } from "next/server";
import {
  historyEnabled,
  historyError,
  historyForm,
  historyLogin,
  historyManage,
  historyQuery,
  historyRedirect,
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

export async function GET(request: NextRequest) {
  if (!historyEnabled()) return historyError(404);
  const session = historySession(request);
  if (!session)
    return request.cookies.has("auth_token") ? signInAgain() : historyLogin(historyManage);
  try {
    const result = await historyQuery("connections", {}, "Bearer " + session);
    if (result.status === 401) return signInAgain();
    if (!result.ok || !Array.isArray(result.data.connections))
      return historyUnavailable(historyManage);
    const rows: string[] = [];
    for (const item of result.data.connections) {
      if (
        !validHistoryId(item?.id) ||
        !validHistoryId(item.workspace_id) ||
        !["ChatGPT", "Claude"].includes(item.host) ||
        typeof item.workspace_name !== "string"
      )
        return historyUnavailable(historyManage);
      rows.push(`<section><h2>${escapeHtml(item.workspace_name)}</h2><p>${escapeHtml(item.host)} can read this workspace's event history.</p><form method="post">
<input type="hidden" name="workspace_id" value="${escapeHtml(item.workspace_id)}"><input type="hidden" name="id" value="${escapeHtml(item.id)}"><button>Disconnect ${escapeHtml(item.host)}</button></form></section>`);
    }
    return historyPage(
      "Event History connections",
      rows.length
        ? rows.join("")
        : "<p>You have no active Event History connections. In ChatGPT or Claude, open your workspace’s configured AllSource Event History connector and choose Connect.</p>"
    );
  } catch {
    return historyUnavailable(historyManage);
  }
}

export async function POST(request: NextRequest) {
  if (!historyEnabled()) return historyError(404);
  if (!historyRequestAllowed(request)) return historyError(403);
  try {
    const form = await historyForm(request, 512);
    if (form?.get("decision") === "signin") return historyLogin(historyManage, true);
    const session = historySession(request);
    if (!session) return signInAgain();
    const id = form?.get("id");
    const workspace_id = form?.get("workspace_id");
    if (!validHistoryId(id) || !validHistoryId(workspace_id)) return historyError(400);
    const result = await historyQuery("revoke", { id, workspace_id }, "Bearer " + session);
    if (result.status === 401) return signInAgain();
    return result.ok ? historyRedirect(historyManage) : historyUnavailable(historyManage);
  } catch {
    return historyUnavailable(historyManage);
  }
}
