import { type NextRequest, NextResponse } from "next/server";
import { customerActionProof } from "@/lib/server/customer-action-proof";
import {
  boundedText,
  privateHeaders,
  productSession,
  sameOrigin,
} from "@/lib/server/customer-agent-http";

const operations = [
  "inspect",
  "share",
  "prepare",
  "read",
  "edit",
  "approve",
  "reject",
  "workspace",
];
const messages: Record<string, string> = {
  access_denied: "A verified product session, current membership and replay consent are required.",
  review_conflict: "Plan, source or decision changed. Refresh the review before deciding.",
  review_expired: "This review or source expired. Prepare a new review.",
  invalid_request: "Review details are incomplete or invalid.",
  review_busy: "Review capacity is busy. Retry the same request shortly.",
  query_quota_exceeded: "Workspace query allowance reached. Review your plan before retrying.",
  rate_limited: "Too many requests. Try again shortly.",
  access_unavailable:
    "Action service unavailable. The request may have completed. Refresh its result before retrying.",
};

function failure(status: number, rawCode: unknown) {
  const code =
    typeof rawCode === "string" && Object.hasOwn(messages, rawCode)
      ? rawCode
      : "access_unavailable";
  return NextResponse.json(
    { error: { code, message: messages[code] } },
    { status, headers: privateHeaders }
  );
}

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ operation: string }> }
) {
  const { operation } = await params;
  if (
    !operations.includes(operation) ||
    !["CUSTOMER_REPLAY_ENABLED", "CUSTOMER_CONNECTIONS_ENABLED", "CUSTOMER_EVIDENCE_ENABLED"].every(
      (key) => process.env[key] === "true"
    )
  )
    return failure(404, "unavailable");
  if (
    request.nextUrl.search ||
    request.headers.has("authorization") ||
    request.headers.has("x-allsource-product-action") ||
    !sameOrigin(request)
  )
    return failure(403, "access_denied");
  const session = request.cookies.get("auth_token")?.value;
  if (!session || !productSession(session)) return failure(401, "access_denied");
  if (request.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json")
    return failure(415, "invalid_request");
  let input: unknown;
  try {
    input = JSON.parse(await boundedText(request.body, 16_384, AbortSignal.timeout(10_000)));
    if (!input || typeof input !== "object" || Array.isArray(input))
      return failure(400, "invalid_request");
  } catch {
    return failure(400, "invalid_request");
  }
  try {
    const deadline = AbortSignal.timeout(45_000);
    const base =
      process.env.QUERY_SERVICE_URL ||
      (process.env.NODE_ENV === "production"
        ? "https://allsource-query.fly.dev"
        : "http://localhost:3902");
    const response = await fetch(new URL(`/api/customer-agent/replay/${operation}`, base), {
      method: "POST",
      body: JSON.stringify(input),
      cache: "no-store",
      redirect: "error",
      signal: deadline,
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${session}`,
        "x-allsource-product-action": customerActionProof(operation, input, session),
      },
    });
    const data = JSON.parse(await boundedText(response.body, 65_536, deadline));
    if (!response.ok) return failure(response.status, data.error?.code);
    return NextResponse.json(data, { headers: privateHeaders });
  } catch {
    return failure(502, "access_unavailable");
  }
}
