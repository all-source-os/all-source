import { type NextRequest, NextResponse } from "next/server";
import { boundedText, privateHeaders, productSession } from "./customer-agent-http";

export const historyHeaders = { ...privateHeaders, "Cache-Control": "private, no-store" };
export const historyRequestCookie = "allsource_history_request";
export const historyReturnCookie = "allsource_history_return";
export const historyConnect = "/event-history/connect";
export const historyManage = "/event-history/connections";
export const historyEnabled = () => process.env.ALLSOURCE_EVENT_HISTORY_ENABLED === "true";

export function historyOrigin() {
  try {
    const raw = process.env.ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN;
    const url = new URL(raw || "");
    return url.origin === raw &&
      (url.protocol === "https:" ||
        (process.env.NODE_ENV !== "production" &&
          ["localhost", "127.0.0.1"].includes(url.hostname)))
      ? url.origin
      : null;
  } catch {
    return null;
  }
}

export function historyRequestAllowed(request: NextRequest) {
  const origin = historyOrigin();
  return (
    historyEnabled() &&
    !!origin &&
    request.headers.get("origin") === origin &&
    request.headers.get("sec-fetch-site") !== "cross-site" &&
    !request.headers.has("authorization")
  );
}

export function historySession(request: NextRequest) {
  const token = request.cookies.get("auth_token")?.value;
  return productSession(token) ? token : undefined;
}

export const cookieOptions = {
  httpOnly: true,
  secure: process.env.NODE_ENV === "production",
  sameSite: "lax" as const,
  maxAge: 600,
};

export function validHistoryId(value: unknown): value is string {
  return typeof value === "string" && /^[A-Za-z0-9_-]{1,128}$/.test(value);
}

export function historyReturnPath(value: string | undefined) {
  if (value === historyConnect || value === historyManage) return value;
  const match = value?.match(/^\/event-history\/workspaces\/([^/]+)\/events\/([^/]+)$/);
  return match && validHistoryId(match[1]) && validHistoryId(match[2]) ? value : undefined;
}

export function historyRedirect(path: string) {
  const origin = historyOrigin();
  if (!origin) return historyError(503);
  return NextResponse.redirect(new URL(path, origin), { status: 303, headers: historyHeaders });
}

export function historyLogin(path: string, clear = false) {
  const next = historyReturnPath(path);
  if (!next) return historyError(400);
  const response = historyRedirect("/login?next=" + encodeURIComponent(next));
  response.cookies.set(historyReturnCookie, next, { ...cookieOptions, path: "/api/auth/callback" });
  if (clear) response.cookies.set("auth_token", "", { ...cookieOptions, path: "/", maxAge: 0 });
  return response;
}

export function historyError(status: number) {
  return NextResponse.json(
    { error: status === 503 ? "temporarily_unavailable" : "invalid_request" },
    { status, headers: historyHeaders }
  );
}

export async function historyForm(request: NextRequest, limit = 8192) {
  if (
    request.headers.get("content-type")?.split(";")[0]?.trim() !==
    "application/x-www-form-urlencoded"
  )
    return null;
  const body = await boundedText(request.body, limit, AbortSignal.timeout(5000));
  const fields = new URLSearchParams(body);
  return new Set(fields.keys()).size === fields.size ? fields : null;
}

export async function historyQuery(
  action: string,
  data: Record<string, unknown> | URLSearchParams,
  auth?: string
) {
  const base =
    process.env.QUERY_SERVICE_URL ||
    (process.env.NODE_ENV === "production"
      ? "https://allsource-query.fly.dev"
      : "http://localhost:3902");
  const form = data instanceof URLSearchParams;
  const deadline = AbortSignal.timeout(action === "evidence" ? 18_000 : 9_000);
  const result = await fetch(new URL("/api/event-history/" + action, base), {
    method: "POST",
    redirect: "error",
    cache: "no-store",
    signal: deadline,
    headers: {
      "content-type": form ? "application/x-www-form-urlencoded" : "application/json",
      ...(auth ? { authorization: auth } : {}),
    },
    body: form ? data.toString() : JSON.stringify(data),
  });
  const json = JSON.parse(await boundedText(result.body, 65_536, deadline));
  return { ok: result.ok, status: result.status, data: json };
}
