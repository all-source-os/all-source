import { createHash } from "node:crypto";
import type { NextRequest } from "next/server";
import {
  cookieOptions,
  historyConnect,
  historyEnabled,
  historyError,
  historyForm,
  historyLogin,
  historyOrigin,
  historyQuery,
  historyRedirect,
  historyRequestAllowed,
  historyRequestCookie,
  historySession,
  validHistoryId,
} from "@/lib/server/event-history-http";
import {
  escapeHtml,
  historyPage,
  historyUnavailable,
  signInAgain,
} from "@/lib/server/event-history-view";

async function inspect(request: NextRequest) {
  const token = request.cookies.get(historyRequestCookie)?.value;
  if (!token || token.length > 3800) return null;
  const result = await historyQuery("oauth/inspect", { request_token: token });
  if (
    !result.ok ||
    !["Claude", "ChatGPT"].includes(result.data.host) ||
    !historyOrigin() ||
    result.data.issuer !== historyOrigin() + "/event-history" ||
    result.data.scope !== "allsource.history.read" ||
    typeof result.data.state !== "string" ||
    typeof result.data.redirect_uri !== "string"
  )
    return null;
  const callback = new URL(result.data.redirect_uri);
  if (callback.protocol !== "https:" || callback.username || callback.password || callback.hash)
    return null;
  return {
    token,
    host: String(result.data.host),
    state: result.data.state,
    callback,
    issuer: result.data.issuer,
  };
}

const cancel =
  '<button class="secondary" name="decision" value="cancel" formnovalidate>Cancel</button>';
function consentBinding(token: string, session: string) {
  return createHash("sha256")
    .update(JSON.stringify([token, session]))
    .digest("base64url");
}
const expired = () =>
  historyPage(
    "Start a new connection",
    "<p>Return to ChatGPT or Claude and choose Connect AllSource Event History to open a fresh request.</p>",
    400
  );

export async function GET(request: NextRequest) {
  if (!historyEnabled()) return historyError(404);
  try {
    const connection = await inspect(request);
    if (!connection) return expired();
    const session = historySession(request);
    if (!session)
      return request.cookies.has("auth_token") ? signInAgain() : historyLogin(historyConnect);
    const result = await historyQuery("workspaces", {}, "Bearer " + session);
    if (result.status === 401) return signInAgain();
    if (!result.ok || !Array.isArray(result.data.workspaces))
      return historyUnavailable(historyConnect);
    const host = connection.host;
    const choices: string[] = [];
    for (const workspace of result.data.workspaces) {
      if (!validHistoryId(workspace?.id) || typeof workspace.name !== "string")
        return historyUnavailable(historyConnect);
      choices.push(
        `<label><input type="radio" name="workspace_id" required value="${escapeHtml(workspace.id)}"> ${escapeHtml(workspace.name)}</label>`
      );
    }
    return historyPage(
      "Connect " + host,
      `<p>${host} will receive the name and ID of the workspace you choose. It can look up event IDs, timestamps, entity names and recorded release changes there. Each history lookup uses one workspace query unit. Viewing a cited event uses one more.</p>
<form method="post"><input type="hidden" name="request" value="${consentBinding(connection.token, session)}">
${choices.length ? `<fieldset><legend>Choose a workspace</legend>${choices.join("")}</fieldset><button name="decision" value="allow">Allow read access</button>` : "<p>Your account has no eligible workspaces. Ask your workspace owner for event-read access and the Event History entitlement.</p>"}${cancel}</form>
<small>Access lasts one hour. To stop future reads sooner, <a href="/event-history/connections">disconnect in AllSource</a>. Records already shared remain in your ${host} conversation.</small>`,
      200,
      connection.callback.origin
    );
  } catch {
    return historyUnavailable(historyConnect);
  }
}

export async function POST(request: NextRequest) {
  if (!historyEnabled()) return historyError(404);
  if (!historyRequestAllowed(request)) return historyError(403);
  try {
    const form = await historyForm(request, 512);
    const decision = form?.get("decision");
    if (decision === "signin") return historyLogin(historyConnect, true);
    if (!form || !["allow", "cancel"].includes(decision || "")) return historyError(400);
    const connection = await inspect(request);
    if (!connection) return expired();
    const session = historySession(request);
    if (!session) return signInAgain();
    if (form.get("request") !== consentBinding(connection.token, session)) return expired();
    const callback = new URL(connection.callback);
    callback.searchParams.set("iss", connection.issuer);
    callback.searchParams.set("state", connection.state);
    if (decision === "cancel") {
      callback.searchParams.set("error", "access_denied");
    } else {
      const workspace = form.get("workspace_id");
      if (!validHistoryId(workspace)) return historyError(400);
      const result = await historyQuery(
        "oauth/decision",
        {
          request_token: connection.token,
          decision: "allow",
          workspace_id: workspace,
        },
        "Bearer " + session
      );
      if (result.status === 401) return signInAgain();
      if (
        !result.ok ||
        result.data.redirect_uri !== connection.callback.href ||
        result.data.state !== connection.state ||
        result.data.issuer !== connection.issuer ||
        typeof result.data.code !== "string" ||
        !result.data.code ||
        result.data.code.length > 4096
      )
        return historyUnavailable(historyConnect);
      callback.searchParams.set("code", result.data.code);
    }
    const response = historyRedirect(callback.href);
    response.cookies.set(historyRequestCookie, "", {
      ...cookieOptions,
      path: historyConnect,
      maxAge: 0,
    });
    return response;
  } catch {
    return historyUnavailable(historyConnect);
  }
}
