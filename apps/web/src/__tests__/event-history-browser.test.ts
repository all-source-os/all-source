import { NextRequest } from "next/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GET as authReturn } from "@/app/api/auth/callback/route";
import { GET as authorize, POST as exchange } from "@/app/api/event-history/oauth/[action]/route";
import { GET as consent, POST as decide } from "@/app/event-history/connect/route";
import { GET as manage, POST as revoke } from "@/app/event-history/connections/route";
import {
  POST as open,
  GET as prepare,
} from "@/app/event-history/workspaces/[workspace]/events/[event]/route";

const origin = "https://review.example";
const workspace = "engineering";
const event = "e1";
const path = "/event-history/workspaces/engineering/events/e1";
const params = { params: Promise.resolve({ workspace, event }) };
const session =
  "h." +
  Buffer.from(
    JSON.stringify({ sub: "p", tenant_id: "other", email_verified: true, provider: "email" })
  ).toString("base64url") +
  ".s";
const cookie = "auth_token=" + session + "; allsource_history_request=r";
const fetcher = vi.fn();
function request(url: string, body?: string, credentials = cookie) {
  return new NextRequest(origin + url, {
    headers: {
      origin,
      cookie: credentials,
      ...(body ? { "content-type": "application/x-www-form-urlencoded" } : {}),
    },
    method: body ? "POST" : "GET",
    body,
  });
}
function answer(data: unknown, status = 200) {
  fetcher.mockResolvedValueOnce(Response.json(data, { status }));
}

describe("self-service Event History browser", () => {
  beforeEach(() => {
    vi.stubEnv("ALLSOURCE_EVENT_HISTORY_ENABLED", "true");
    vi.stubEnv("ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN", origin);
    vi.stubGlobal("fetch", fetcher);
  });
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
    fetcher.mockReset();
  });

  it.each([
    ["ChatGPT", "https://chatgpt.com/reviewer-callback"],
    ["Claude", "https://claude.ai/api/mcp/auth_callback"],
  ])("connects %s after explicit workspace choice", async (host, callback) => {
    const inspected = {
      host,
      redirect_uri: callback,
      issuer: origin + "/event-history",
      state: "opaque",
      scope: "allsource.history.read",
    };
    answer({ request_token: "r" });
    const started = await authorize(request("/api/event-history/oauth/authorize?state=opaque"), {
      params: Promise.resolve({ action: "authorize" }),
    });
    expect(started.headers.get("location")).toBe(origin + "/event-history/connect");
    expect(started.headers.get("set-cookie")).toContain("HttpOnly");
    expect(started.headers.get("set-cookie")).toContain("Path=/event-history/connect");
    answer(inspected);
    answer({
      workspaces: [
        { id: workspace, name: "Engineering <team>" },
        { id: "support", name: "Support" },
      ],
    });
    const page = await consent(request("/event-history/connect"));
    const markup = await page.text();
    const binding = markup.match(/name="request" value="([^"]+)"/)?.[1];
    expect(binding).toBeTruthy();
    expect(markup).toContain("Engineering &lt;team&gt;");
    expect(markup).toContain('value="support"');
    expect(markup).not.toContain("checked");
    expect(page.headers.get("cache-control")).toBe("private, no-store");
    expect(page.headers.get("referrer-policy")).toBe("strict-origin");
    answer(inspected);
    answer({ ...inspected, code: "one-use" });
    const decision = await decide(
      request(
        "/event-history/connect",
        `decision=allow&workspace_id=engineering&request=${binding}`
      )
    );
    expect(new URL(decision.headers.get("location")!).searchParams.get("code")).toBe("one-use");
    expect(fetcher.mock.lastCall?.[1].body).toBe(
      JSON.stringify({ request_token: "r", decision: "allow", workspace_id: workspace })
    );
    answer(inspected);
    const canceled = await decide(
      request("/event-history/connect", `decision=cancel&request=${binding}`)
    );
    expect(new URL(canceled.headers.get("location")!).searchParams.get("error")).toBe(
      "access_denied"
    );
    expect(fetcher.mock.lastCall?.[0].pathname).toBe("/api/event-history/oauth/inspect");
    answer(inspected);
    const stale = await decide(
      request(
        "/event-history/connect",
        `decision=allow&workspace_id=engineering&request=${binding}`,
        cookie.replace("request=r", "request=other-tab")
      )
    );
    expect(stale.status).toBe(400);
    expect(fetcher.mock.lastCall?.[0].pathname).toBe("/api/event-history/oauth/inspect");
  });

  it("preserves citation through product login and rejects external return cookie", async () => {
    const login = await prepare(request(path, undefined, ""), params);
    expect(new URL(login.headers.get("location")!).searchParams.get("next")).toBe(path);
    expect(fetcher).not.toHaveBeenCalled();
    answer({});
    const returned = await authReturn(
      request("/api/auth/callback?token=synthetic", undefined, "allsource_history_return=" + path)
    );
    expect(returned.headers.get("location")).toBe(origin + path);
    answer({});
    const forged = await authReturn(
      request(
        "/api/auth/callback?token=synthetic",
        undefined,
        "allsource_history_return=//another.example"
      )
    );
    expect(forged.headers.get("location")).toBe(origin + "/dashboard");
  });

  it("opens only an explicit read and displays the matching cited event", async () => {
    answer({ intent: "bound" });
    const shell = await prepare(request(path), params);
    expect(await shell.text()).toContain("one workspace query unit");
    expect(fetcher.mock.calls.map((c) => c[0].pathname)).toEqual([
      "/api/event-history/evidence-intent",
    ]);
    answer({
      workspace: { id: workspace, name: "Engineering" },
      event: {
        id: event,
        kind: "rolled_back",
        entity: "checkout",
        timestamp: "2026-11-26T16:00:00.123456Z",
        from_version: "2.4",
        to_version: "2.3",
        payload: "PRIVATE-CANARY",
      },
    });
    const opened = await open(request(path, "intent=bound"), params);
    const text = await opened.text();
    expect(text).toContain("Rolled back from 2.4 to 2.3");
    expect(text).toContain(event);
    expect(text).toContain("2026-11-26T16:00:00.123456Z");
    expect(text).not.toContain("PRIVATE-CANARY");
    expect(text).not.toContain("<script");
    expect(fetcher.mock.lastCall?.[1].headers.authorization).toBe("Bearer " + session);
    answer({ workspace: { id: "other", name: "Private team" }, event: { id: event } });
    expect(await (await open(request(path, "intent=bound"), params)).text()).not.toContain(
      "Private team"
    );
    answer({}, 409);
    expect(await (await open(request(path, "intent=bound"), params)).text()).toContain(
      "Start a new read"
    );
  });

  it("refuses foreign origin decisions, disabled routes and cross-workspace evidence", async () => {
    const foreign = request(path, "intent=bound");
    foreign.headers.set("origin", "https://foreign.example");
    expect((await open(foreign, params)).status).toBe(403);
    expect(fetcher).not.toHaveBeenCalled();
    answer({}, 403);
    const refused = await open(request(path, "intent=bound"), params);
    answer({}, 404);
    const missing = await open(request(path, "intent=bound"), params);
    expect(await refused.text()).toBe(await missing.text());
    vi.stubEnv("ALLSOURCE_EVENT_HISTORY_ENABLED", "false");
    expect((await prepare(request(path), params)).status).toBe(404);
  });

  it("forwards token client authentication without forwarding browser cookies", async () => {
    const req = request("/api/event-history/oauth/token", "code=synthetic");
    req.headers.set("authorization", "Basic YTpi");
    answer({
      access_token: "issued",
      token_type: "Bearer",
      expires_in: 900,
      scope: "allsource.history.read",
    });
    expect((await exchange(req, { params: Promise.resolve({ action: "token" }) })).status).toBe(
      200
    );
    expect(fetcher.mock.lastCall?.[1].headers).toEqual({
      "content-type": "application/x-www-form-urlencoded",
      authorization: "Basic YTpi",
    });
  });

  it("disconnects only the selected connection and returns to its list", async () => {
    answer({
      connections: [
        { id: "g1", host: "Claude", workspace_id: workspace, workspace_name: "Engineering" },
      ],
    });
    expect(await (await manage(request("/event-history/connections"))).text()).toContain(
      "Disconnect Claude"
    );
    answer({});
    const result = await revoke(
      request("/event-history/connections", "id=g1&workspace_id=engineering")
    );
    expect(result.headers.get("location")).toBe(origin + "/event-history/connections");
    expect(fetcher.mock.lastCall?.[1].body).toBe(
      JSON.stringify({ id: "g1", workspace_id: workspace })
    );
  });
});
