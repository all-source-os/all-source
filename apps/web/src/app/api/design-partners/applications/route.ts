import { type NextRequest, NextResponse } from "next/server";
import { clientIpForUpstream, readCappedText, UPSTREAM_TIMEOUT_MS } from "@/lib/upstream-request";

const MAX_BODY_BYTES = 16 * 1024;

function controlPlaneUrl(): string {
  return process.env.CONTROL_PLANE_INTERNAL_URL || "http://localhost:3901";
}

export async function POST(request: NextRequest): Promise<NextResponse> {
  const body = await readCappedText(request, MAX_BODY_BYTES);
  if (body === null) {
    return NextResponse.json(
      { error: "payload_too_large", message: "Application is too large." },
      { status: 413 }
    );
  }

  const headers: Record<string, string> = { "content-type": "application/json" };
  const visitorIp = clientIpForUpstream(request.headers);
  if (visitorIp) headers["x-forwarded-for"] = visitorIp;

  try {
    const response = await fetch(`${controlPlaneUrl()}/api/v1/design-partners/applications`, {
      method: "POST",
      headers,
      body,
      cache: "no-store",
      signal: AbortSignal.timeout(UPSTREAM_TIMEOUT_MS),
    });
    const responseBody = await response.text();
    const responseHeaders: Record<string, string> = {
      "content-type": response.headers.get("content-type") || "application/json",
      "cache-control": "no-store",
    };
    const retryAfter = response.headers.get("retry-after");
    if (retryAfter) responseHeaders["retry-after"] = retryAfter;
    return new NextResponse(responseBody, { status: response.status, headers: responseHeaders });
  } catch {
    return NextResponse.json(
      {
        error: "application_unavailable",
        message: "Applications are temporarily unavailable. Please try again later.",
      },
      { status: 503, headers: { "cache-control": "no-store" } }
    );
  }
}
