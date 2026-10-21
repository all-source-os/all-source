import { type NextRequest, NextResponse } from "next/server";
import { clientIpForUpstream, readCappedText, UPSTREAM_TIMEOUT_MS } from "@/lib/upstream-request";

const MAX_BODY_BYTES = 4 * 1024;
const PLACEMENTS = new Set(["home", "prime"]);
const EMAIL_PATTERN = /^[^\s@<>()",;:]+@[^\s@<>()",;:]+\.[^\s@<>()",;:]+$/;
const INVALID = {
  error: "invalid_subscription",
  message: "Enter a valid email and accept the consent text.",
};
const NO_STORE = { "cache-control": "no-store" };

interface CampaignSource {
  source?: string;
  medium?: string;
  campaign?: string;
  content?: string;
}

interface SubscriptionBody {
  email?: unknown;
  consent?: unknown;
  placement?: unknown;
  campaign_source?: unknown;
  cf_turnstile_response?: unknown;
}

function controlPlaneUrl(): string {
  return process.env.CONTROL_PLANE_INTERNAL_URL || "http://localhost:3901";
}

function campaignSource(value: unknown): CampaignSource {
  if (!value || typeof value !== "object") return {};
  const source: CampaignSource = {};
  for (const key of ["source", "medium", "campaign", "content"] as const) {
    const field = (value as Record<string, unknown>)[key];
    if (typeof field === "string" && field.length <= 100) source[key] = field;
  }
  return source;
}

export async function POST(request: NextRequest): Promise<NextResponse> {
  const raw = await readCappedText(request, MAX_BODY_BYTES);
  if (raw === null) {
    return NextResponse.json(INVALID, { status: 413, headers: NO_STORE });
  }

  let body: SubscriptionBody;
  try {
    body = JSON.parse(raw) as SubscriptionBody;
  } catch {
    return NextResponse.json(INVALID, { status: 400, headers: NO_STORE });
  }
  const email = typeof body.email === "string" ? body.email.trim().toLowerCase() : "";
  if (
    !email ||
    email.length > 254 ||
    !EMAIL_PATTERN.test(email) ||
    body.consent !== true ||
    typeof body.placement !== "string" ||
    !PLACEMENTS.has(body.placement)
  ) {
    return NextResponse.json(INVALID, { status: 400, headers: NO_STORE });
  }

  const headers: Record<string, string> = { "content-type": "application/json" };
  const visitorIp = clientIpForUpstream(request.headers);
  if (visitorIp) headers["x-forwarded-for"] = visitorIp;

  try {
    const response = await fetch(`${controlPlaneUrl()}/api/v1/changelog/subscriptions`, {
      method: "POST",
      headers,
      body: JSON.stringify({
        email,
        consent: true,
        placement: body.placement,
        campaign_source: campaignSource(body.campaign_source),
        ...(typeof body.cf_turnstile_response === "string" && {
          cf_turnstile_response: body.cf_turnstile_response,
        }),
      }),
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
        error: "subscription_unavailable",
        message: "Signups are temporarily unavailable. Please try again later.",
      },
      { status: 503, headers: NO_STORE }
    );
  }
}
