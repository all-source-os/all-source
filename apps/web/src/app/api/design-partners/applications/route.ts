import { type NextRequest, NextResponse } from "next/server";

const MAX_BODY_BYTES = 16 * 1024;

function controlPlaneUrl(): string {
  return process.env.CONTROL_PLANE_INTERNAL_URL || "http://localhost:3901";
}

export async function POST(request: NextRequest): Promise<NextResponse> {
  const body = await request.text();
  if (new TextEncoder().encode(body).byteLength > MAX_BODY_BYTES) {
    return NextResponse.json(
      { error: "payload_too_large", message: "Application is too large." },
      { status: 413 }
    );
  }

  const headers: Record<string, string> = { "content-type": "application/json" };
  const forwardedFor = request.headers.get("x-forwarded-for");
  if (forwardedFor) {
    headers["x-forwarded-for"] = forwardedFor.split(",", 1)[0]?.trim() || "";
  }

  try {
    const response = await fetch(`${controlPlaneUrl()}/api/v1/design-partners/applications`, {
      method: "POST",
      headers,
      body,
      cache: "no-store",
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
