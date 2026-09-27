import { type NextRequest, NextResponse } from "next/server";
import {
  boundedText,
  privateHeaders,
  productSession,
  queryConnection,
  sameOrigin,
} from "@/lib/server/customer-agent-http";

const evidenceOperations = ["inspect-run", "share", "workspace", "revoke-source", "read-review"];
const messages: Record<string, string> = {
  connection_limit: "Connection limit reached. Revoke an unused connection or try again tomorrow.",
  access_denied: "A verified workspace session and eligible connection are required.",
  invalid_request: "Check your connection settings and consent, then retry.",
  invalid_source: "This run could not be shared at that revision. Inspect it again before sharing.",
  rate_limited: "Too many requests. Try again shortly.",
  review_busy: "Review capacity is busy. Retry the same request shortly.",
  review_conflict:
    "The request or billing period changed. Refresh saved work before starting again.",
  review_expired: "This request expired. Refresh saved work before starting again.",
  query_quota_exceeded: "Workspace query allowance reached. Review your plan before retrying.",
};

function failure(status: number, rawCode: unknown) {
  const code =
    typeof rawCode === "string" && Object.hasOwn(messages, rawCode)
      ? rawCode
      : "access_unavailable";
  return NextResponse.json(
    {
      error: {
        code,
        message:
          messages[code] ||
          "Connection service unavailable. The request may have completed. Retry the same request or refresh saved work.",
      },
    },
    { status, headers: privateHeaders }
  );
}

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ operation: string }> }
) {
  const { operation } = await params;
  const evidence = process.env.CUSTOMER_EVIDENCE_ENABLED === "true";
  if (
    process.env.CUSTOMER_CONNECTIONS_ENABLED !== "true" ||
    !["list", "create", "revoke", ...evidenceOperations].includes(operation) ||
    (evidenceOperations.includes(operation) && !evidence)
  )
    return failure(404, "unavailable");
  if (request.nextUrl.search || request.headers.has("authorization") || !sameOrigin(request))
    return failure(403, "access_denied");
  const token = request.cookies.get("auth_token")?.value;
  if (!productSession(token)) return failure(401, "access_denied");
  if (request.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json")
    return failure(415, "invalid_request");
  let body: string;
  try {
    body = await boundedText(request.body, 4096, AbortSignal.timeout(10_000));
  } catch {
    return failure(413, "invalid_request");
  }
  try {
    const input = JSON.parse(body);
    if (!input || typeof input !== "object" || Array.isArray(input))
      return failure(400, "invalid_request");
    if (operation === "create" && input.consent?.version === "review-evidence-v2" && !evidence)
      return failure(404, "unavailable");
  } catch {
    return failure(400, "invalid_request");
  }
  try {
    const result = await queryConnection(`connections/${operation}`, body, token, false, 45_000);
    if (!result.ok) return failure(result.status, result.data.error?.code);
    return NextResponse.json(result.data, { headers: privateHeaders });
  } catch {
    return failure(502, "access_unavailable");
  }
}
