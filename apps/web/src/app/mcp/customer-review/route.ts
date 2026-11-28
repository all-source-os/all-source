import type { NextRequest } from "next/server";
import { forwardMcp } from "@/lib/server/forward-mcp";
import { remoteEnabled, remoteIssuer } from "@/lib/server/customer-agent-http";

function forward(request: NextRequest) {
  return forwardMcp(request, {
    enabled: remoteEnabled(),
    origin: remoteIssuer(),
    upstream: process.env.CUSTOMER_REVIEW_HTTP_URL,
    path: "/mcp/customer-review",
    origins: process.env.CUSTOMER_REVIEW_ORIGINS,
    metadata: "/.well-known/oauth-protected-resource",
    scope: "allsource.review",
  });
}

export const POST = forward;
export const GET = forward;
export const DELETE = forward;
