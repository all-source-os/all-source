import { NextRequest } from "next/server";
import { afterEach, expect, it, vi } from "vitest";
import { GET as metadata } from "@/app/.well-known/oauth-protected-resource/mcp/event-history/route";
import { POST } from "@/app/mcp/event-history/route";

const origin = "https://review.example.test";
const path = "/mcp/event-history";
const session = "4e9466d6-fe88-42f4-8a2b-fa78ad7eab51";

afterEach(() => {
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

it("keeps the listener disabled until the reviewer configures it", async () => {
  vi.stubEnv("ALLSOURCE_EVENT_HISTORY_ENABLED", "false");
  expect((await POST(new NextRequest(origin + path, { method: "POST" }))).status).toBe(404);
  expect((await metadata()).status).toBe(404);
});

it("advertises the history issuer and preserves MCP sessions without browser credentials", async () => {
  vi.stubEnv("ALLSOURCE_EVENT_HISTORY_ENABLED", "true");
  vi.stubEnv("ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN", origin);
  vi.stubEnv("ALLSOURCE_EVENT_HISTORY_HTTP_URL", "http://127.0.0.1:14904");
  vi.stubEnv("QUERY_SERVICE_URL", "http://127.0.0.1:14902");
  const resource = { resource: origin + path, authorization_servers: [origin + "/event-history"] };
  const upstream = vi
    .fn<typeof fetch>()
    .mockResolvedValueOnce(Response.json(resource))
    .mockResolvedValueOnce(
      Response.json(
        { jsonrpc: "2.0", id: 1, result: {} },
        { headers: { "mcp-session-id": session } }
      )
    )
    .mockResolvedValueOnce(new Response(null, { status: 202 }));
  vi.stubGlobal("fetch", upstream);
  const challenge = await POST(new NextRequest(origin + path, { method: "POST" }));
  expect(challenge.status).toBe(401);
  expect(challenge.headers.get("www-authenticate")).toContain(
    origin + "/.well-known/oauth-protected-resource" + path
  );
  expect(await (await metadata()).json()).toEqual(resource);
  expect(upstream.mock.calls[0]?.[0].toString()).toBe(
    "http://127.0.0.1:14902/api/event-history/oauth/resource"
  );
  const post = (method: string, extra: Record<string, string>) =>
    POST(
      new NextRequest(origin + path, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          authorization: "Bearer synthetic",
          cookie: "auth_token=private",
          ...extra,
        },
        body: JSON.stringify({ jsonrpc: "2.0", method }),
      })
    );
  const init = await post("initialize", {});
  expect(init.headers.get("mcp-session-id")).toBe(session);
  const ack = await post("notifications/initialized", { "mcp-session-id": session });
  expect(ack.status).toBe(202);
  const headers = new Headers(upstream.mock.calls[2]?.[1]?.headers);
  expect(headers.get("mcp-session-id")).toBe(session);
  expect(headers.has("cookie")).toBe(false);
});
