import type { NextRequest } from "next/server";
import { decodeJwtPayload } from "@/lib/server/session-token";

export const privateHeaders = {
  "Cache-Control": "no-store",
  Pragma: "no-cache",
  "Referrer-Policy": "no-referrer",
  "X-Robots-Tag": "noindex, nofollow",
};
export const remoteRequestCookie = "customer_oauth_request";
export const claudeCallback = "https://claude.ai/api/mcp/auth_callback";
export const remoteEnabled = () =>
  process.env.CUSTOMER_REMOTE_ENABLED === "true" &&
  process.env.CUSTOMER_CONNECTIONS_ENABLED === "true";

export function connectionOrigin(request: NextRequest) {
  const configured = process.env.NEXT_PUBLIC_APP_URL;
  if (configured) {
    try {
      return new URL(configured).origin;
    } catch {
      return null;
    }
  }
  return process.env.NODE_ENV === "production" ? null : request.nextUrl.origin;
}

export function remoteIssuer() {
  const configured = process.env.CUSTOMER_OAUTH_ISSUER;
  try {
    const url = new URL(configured || "");
    return url.protocol === "https:" && url.origin === configured ? configured : null;
  } catch {
    return null;
  }
}

export function sameOrigin(request: NextRequest) {
  const origin = connectionOrigin(request);
  return (
    !!origin &&
    request.headers.get("origin") === origin &&
    request.headers.get("sec-fetch-site") !== "cross-site"
  );
}

export function productSession(token: string | undefined) {
  const claims = token && token.length <= 8192 ? decodeJwtPayload(token) : null;
  if (
    !claims ||
    typeof claims !== "object" ||
    Array.isArray(claims) ||
    typeof claims.sub !== "string" ||
    typeof claims.tenant_id !== "string" ||
    claims.email_verified !== true ||
    !["google", "github", "email"].includes(String(claims.provider)) ||
    claims.is_api_key ||
    claims.view_as ||
    claims.is_demo ||
    claims.core_api_key ||
    claims.api_key ||
    claims.act_as
  )
    return null;
  return claims;
}

export async function boundedText(
  stream: ReadableStream<Uint8Array> | null,
  limit: number,
  signal: AbortSignal
) {
  if (!stream) return "";
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  const abort = () => {
    void reader.cancel().catch(() => {});
  };
  signal.addEventListener("abort", abort, { once: true });
  try {
    while (true) {
      signal.throwIfAborted();
      const { done, value } = await reader.read();
      signal.throwIfAborted();
      if (done) return Buffer.concat(chunks).toString("utf8");
      size += value.byteLength;
      if (size > limit) throw new Error("size limit");
      chunks.push(value);
    }
  } finally {
    signal.removeEventListener("abort", abort);
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

export async function queryConnection(path: string, body?: string, token?: string, form = false) {
  const deadline = AbortSignal.timeout(9_000);
  const base =
    process.env.QUERY_SERVICE_URL ||
    (process.env.NODE_ENV === "production"
      ? "https://allsource-query.fly.dev"
      : "http://localhost:3902");
  const response = await fetch(new URL(`/api/customer-agent/${path}`, base), {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "content-type": form ? "application/x-www-form-urlencoded" : "application/json",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body,
    cache: "no-store",
    redirect: "error",
    signal: deadline,
  });
  const data = JSON.parse(await boundedText(response.body, 65_536, deadline));
  return { status: response.status, ok: response.ok, data };
}
