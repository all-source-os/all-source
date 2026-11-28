import type { NextRequest } from "next/server";
import { forwardMcp } from "@/lib/server/forward-mcp";
import { historyEnabled, historyOrigin } from "@/lib/server/event-history-http";

function forward(request: NextRequest) {
  return forwardMcp(request, {
    enabled: historyEnabled(),
    origin: historyOrigin(),
    upstream: process.env.ALLSOURCE_EVENT_HISTORY_HTTP_URL,
    path: "/mcp/event-history",
    origins: process.env.ALLSOURCE_EVENT_HISTORY_ORIGINS,
    metadata: "/.well-known/oauth-protected-resource/mcp/event-history",
    scope: "allsource.history.read",
    sessions: true,
  });
}

export const POST = forward;
export const GET = forward;
export const DELETE = forward;
