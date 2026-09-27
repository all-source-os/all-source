import { type NextRequest, NextResponse } from "next/server";
import { decodeJwtPayload } from "@/lib/server/session-token";

const noStore = { "Cache-Control": "no-store" };
const messages: Record<string, string> = {
  connection_limit: "Connection limit reached. Revoke an unused connection or try again tomorrow.",
  access_denied: "A verified workspace session and eligible plan are required.",
  invalid_request: "Check your connection settings and consent, then retry.",
  rate_limited: "Too many requests. Try again shortly.",
};
function failure(status: number, code: string) {
  return NextResponse.json(
    {
      error: { code, message: messages[code] || "Connection service unavailable. Retry shortly." },
    },
    { status, headers: noStore }
  );
}

async function boundedText(
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

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ operation: string }> }
) {
  const { operation } = await params;
  if (
    process.env.CUSTOMER_CONNECTIONS_ENABLED !== "true" ||
    !["list", "create", "revoke"].includes(operation)
  )
    return failure(404, "unavailable");
  if (request.nextUrl.search || request.headers.has("authorization"))
    return failure(403, "access_denied");
  const token = request.cookies.get("auth_token")?.value;
  const claims = token ? decodeJwtPayload(token) : null;
  if (
    !token ||
    token.length > 8192 ||
    !claims ||
    typeof claims !== "object" ||
    Array.isArray(claims) ||
    typeof claims.sub !== "string" ||
    claims.email_verified !== true ||
    !["google", "github", "email"].includes(String(claims.provider)) ||
    claims.is_api_key ||
    claims.view_as ||
    claims.is_demo ||
    claims.core_api_key ||
    claims.api_key
  )
    return failure(401, "access_denied");
  const configured = process.env.NEXT_PUBLIC_APP_URL;
  const origin = configured
    ? new URL(configured).origin
    : process.env.NODE_ENV === "production"
      ? null
      : request.nextUrl.origin;
  if (
    !origin ||
    request.headers.get("origin") !== origin ||
    request.headers.get("sec-fetch-site") === "cross-site"
  )
    return failure(403, "access_denied");
  if (request.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json")
    return failure(415, "invalid_request");
  let body: string;
  try {
    body = await boundedText(request.body, 4096, AbortSignal.timeout(10_000));
  } catch {
    return failure(413, "invalid_request");
  }
  try {
    const deadline = AbortSignal.timeout(45_000);
    const base =
      process.env.QUERY_SERVICE_URL ||
      (process.env.NODE_ENV === "production"
        ? "https://allsource-query.fly.dev"
        : "http://localhost:3902");
    const response = await fetch(new URL(`/api/customer-agent/connections/${operation}`, base), {
      method: "POST",
      headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
      body,
      cache: "no-store",
      redirect: "error",
      signal: deadline,
    });
    const result = JSON.parse(await boundedText(response.body, 65536, deadline));
    if (!response.ok) return failure(response.status, result.error?.code || "access_unavailable");
    return NextResponse.json(result, { headers: noStore });
  } catch {
    return failure(502, "access_unavailable");
  }
}
