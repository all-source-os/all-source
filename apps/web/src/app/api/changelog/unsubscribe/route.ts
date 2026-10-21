import { type NextRequest, NextResponse } from "next/server";
import { clientIpForUpstream, readCappedText, UPSTREAM_TIMEOUT_MS } from "@/lib/upstream-request";

const MAX_BODY_BYTES = 1024;
const TOKEN_PATTERN = /^[a-f0-9]{64}$/;
const ID_PATTERN = /^[a-f0-9]{32}$/;
const NO_STORE = { "cache-control": "no-store" };
const INVALID = { error: "invalid_link", message: "This unsubscribe link is not valid." };

function controlPlaneUrl(): string {
  return process.env.CONTROL_PLANE_INTERNAL_URL || "http://localhost:3901";
}

export async function POST(request: NextRequest): Promise<NextResponse> {
  const raw = await readCappedText(request, MAX_BODY_BYTES);
  if (raw === null) return NextResponse.json(INVALID, { status: 413, headers: NO_STORE });

  let body: { id?: unknown; token?: unknown };
  try {
    body = JSON.parse(raw);
  } catch {
    return NextResponse.json(INVALID, { status: 400, headers: NO_STORE });
  }
  const id = typeof body.id === "string" ? body.id : "";
  const token = typeof body.token === "string" ? body.token : "";
  if (!ID_PATTERN.test(id) || !TOKEN_PATTERN.test(token)) {
    return NextResponse.json(INVALID, { status: 400, headers: NO_STORE });
  }

  const headers: Record<string, string> = { "content-type": "application/json" };
  const visitorIp = clientIpForUpstream(request.headers);
  if (visitorIp) headers["x-forwarded-for"] = visitorIp;

  try {
    const response = await fetch(`${controlPlaneUrl()}/api/v1/changelog/unsubscribe`, {
      method: "POST",
      headers,
      body: JSON.stringify({ id, token }),
      cache: "no-store",
      signal: AbortSignal.timeout(UPSTREAM_TIMEOUT_MS),
    });
    const responseHeaders: Record<string, string> = {
      "content-type": response.headers.get("content-type") || "application/json",
      ...NO_STORE,
    };
    const retryAfter = response.headers.get("retry-after");
    if (retryAfter) responseHeaders["retry-after"] = retryAfter;
    return new NextResponse(await response.text(), {
      status: response.status,
      headers: responseHeaders,
    });
  } catch {
    return NextResponse.json(
      {
        error: "unsubscribe_unavailable",
        message: "We couldn't record that right now. Please try the link again shortly.",
      },
      { status: 503, headers: NO_STORE }
    );
  }
}
