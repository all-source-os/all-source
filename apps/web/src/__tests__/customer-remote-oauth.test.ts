// @vitest-environment node
import { NextRequest } from "next/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { GET as callback } from "@/app/api/auth/callback/route";
import { GET, POST } from "@/app/api/customer-agent/oauth/[action]/route";
import { POST as mcp } from "@/app/mcp/customer-review/route";

const origin = "https://www.example.test";
const redirect = "https://claude.ai/api/mcp/auth_callback";
const pending = "encrypted-synthetic-request";
const inspected = { redirect_uri: redirect, issuer: origin, state: "synthetic-state" };
const session = `header.${Buffer.from(JSON.stringify({ sub: "oauth:google:one", tenant_id: "tenant-one", provider: "google", email_verified: true })).toString("base64url")}.signature`;
const context = (action: string) => ({ params: Promise.resolve({ action }) });
function decision(body = "decision=allow&consent=yes", headers = {}) {
  return new NextRequest(`${origin}/api/customer-agent/oauth/decision`, {
    method: "POST",
    body,
    headers: {
      origin,
      "content-type": "application/x-www-form-urlencoded",
      cookie: `auth_token=${session}; customer_oauth_request=${pending}`,
      ...headers,
    },
  });
}
describe("remote OAuth browser boundary", () => {
  beforeEach(() => {
    vi.stubEnv("CUSTOMER_REMOTE_ENABLED", "true");
    vi.stubEnv("CUSTOMER_CONNECTIONS_ENABLED", "true");
    vi.stubEnv("CUSTOMER_OAUTH_ISSUER", origin);
    vi.stubEnv("NEXT_PUBLIC_APP_URL", origin);
    vi.stubEnv("QUERY_SERVICE_URL", "http://127.0.0.1:3902");
    vi.stubEnv("CUSTOMER_REVIEW_HTTP_URL", "http://127.0.0.1:3904");
  });
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });

  it("moves the bounded authorization request into an HttpOnly cookie and clean consent URL", async () => {
    const fetcher = vi.fn().mockResolvedValue(Response.json({ request_token: pending }));
    vi.stubGlobal("fetch", fetcher);
    const response = await GET(
      new NextRequest(`${origin}/api/customer-agent/oauth/authorize?state=private`),
      context("authorize")
    );
    expect(response.headers.get("location")).toBe(`${origin}/connect/claude`);
    expect(response.cookies.get("customer_oauth_request")).toMatchObject({
      value: pending,
      httpOnly: true,
      sameSite: "lax",
      maxAge: 600,
    });
    expect(response.headers.get("referrer-policy")).toBe("no-referrer");
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(fetcher.mock.calls[0]?.[1]).toMatchObject({
      body: '{"state":"private"}',
      redirect: "error",
      cache: "no-store",
    });
    fetcher.mockClear();
    expect(
      (
        await GET(
          new NextRequest(`${origin}/api/customer-agent/oauth/authorize?state=a&state=b`),
          context("authorize")
        )
      ).status
    ).toBe(400);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("sends only signed product session upstream after same-origin human consent", async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValueOnce(Response.json(inspected))
      .mockResolvedValueOnce(Response.json({ ...inspected, code: "synthetic-code" }));
    vi.stubGlobal("fetch", fetcher);
    const response = await POST(decision(), context("decision"));
    const destination = new URL(response.headers.get("location") || "");
    expect(destination.origin + destination.pathname).toBe(redirect);
    expect([...destination.searchParams.keys()].sort()).toEqual(["code", "iss", "state"]);
    expect(response.cookies.get("customer_oauth_request")?.maxAge).toBe(0);
    expect(fetcher.mock.calls[1]?.[1]).toMatchObject({
      headers: { authorization: `Bearer ${session}` },
      body: JSON.stringify({
        request_token: pending,
        consent: { accepted: true, version: "review-metadata-v1" },
      }),
    });
    expect(fetcher.mock.calls[1]?.[1].headers.cookie).toBeUndefined();
  });

  it.each([
    { origin: "https://evil.test" },
    { origin: "" },
    { "sec-fetch-site": "cross-site" },
    { authorization: "Bearer model" },
  ])("rejects forged consent %j", async (headers) => {
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    expect([400, 403]).toContain(
      (await POST(decision(undefined, headers), context("decision"))).status
    );
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("cancel never mints a grant; sign-in clears stale session but preserves pending request", async () => {
    const fetcher = vi.fn().mockImplementation(() => Promise.resolve(Response.json(inspected)));
    vi.stubGlobal("fetch", fetcher);
    const canceled = await POST(decision("decision=deny"), context("decision"));
    const url = new URL(canceled.headers.get("location") || "");
    expect(url.searchParams.get("error")).toBe("access_denied");
    expect(url.searchParams.has("code")).toBe(false);
    expect(fetcher).toHaveBeenCalledTimes(1);
    const signin = await POST(decision("decision=signin"), context("decision"));
    expect(signin.headers.get("location")).toBe(`${origin}/login?next=%2Fconnect%2Fclaude`);
    expect(signin.cookies.get("auth_token")?.maxAge).toBe(0);
    expect(signin.cookies.has("customer_oauth_request")).toBe(false);
  });

  it("rejects callback substitution and never redirects raw upstream errors", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(Response.json({ ...inspected, redirect_uri: "https://evil.test" }))
    );
    const response = await POST(decision(), context("decision"));
    expect(response.headers.get("location")).toBe(`${origin}/connect/claude?error=expired`);
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(Response.json(inspected)));
    const denied = await POST(decision("decision=allow"), context("decision"));
    expect(denied.headers.get("location")).toBe(`${origin}/connect/claude?error=denied`);
  });

  it("token exchange strips browser credentials; login callback returns to pending consent", async () => {
    const fetcher = vi
      .fn()
      .mockImplementation(() =>
        Promise.resolve(Response.json({ access_token: "synthetic-envelope", token_type: "Bearer" }))
      );
    vi.stubGlobal("fetch", fetcher);
    const request = new NextRequest(`${origin}/api/customer-agent/oauth/token`, {
      method: "POST",
      body: "code=synthetic",
      headers: {
        "content-type": "application/x-www-form-urlencoded",
        cookie: `auth_token=${session}`,
      },
    });
    expect((await POST(request, context("token"))).status).toBe(200);
    expect(fetcher.mock.calls[0]?.[1].headers.authorization).toBeUndefined();
    const loggedIn = await callback(
      new NextRequest(`${origin}/api/auth/callback?token=synthetic-session`, {
        headers: { cookie: `customer_oauth_request=${pending}` },
      })
    );
    expect(loggedIn.headers.get("location")).toBe(`${origin}/connect/claude`);
  });

  it("MCP proxy ignores browser sessions, strips cookies, preserves challenges and notifications", async () => {
    const fetcher = vi
      .fn()
      .mockImplementation(() => Promise.resolve(new Response(null, { status: 202 })));
    vi.stubGlobal("fetch", fetcher);
    const request = (authorization?: string) =>
      new NextRequest(`${origin}/mcp/customer-review`, {
        method: "POST",
        body: '{"jsonrpc":"2.0","method":"notifications/initialized"}',
        headers: {
          cookie: `auth_token=${session}`,
          "content-type": "application/json",
          ...(authorization ? { authorization } : {}),
        },
      });
    const denied = await mcp(request());
    expect(denied.status).toBe(401);
    expect(denied.headers.get("www-authenticate")).toContain(
      "/.well-known/oauth-protected-resource"
    );
    expect(fetcher).not.toHaveBeenCalled();
    const response = await mcp(request("Bearer synthetic-envelope"));
    expect(response.status).toBe(202);
    expect(await response.text()).toBe("");
    const headers = fetcher.mock.calls[0]?.[1].headers as Headers;
    expect(headers.get("cookie")).toBeNull();
    expect(headers.get("authorization")).toBe("Bearer synthetic-envelope");
  });
});
