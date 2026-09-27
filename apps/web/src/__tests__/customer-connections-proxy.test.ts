// @vitest-environment node
import { NextRequest } from "next/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { POST as genericPost } from "@/app/api/[...path]/route";
import { POST } from "@/app/api/customer-agent/connections/[operation]/route";
import { POST as v1Post } from "@/app/api/v1/[...path]/route";

const origin = "https://www.all-source.xyz";
function token(extra = {}) {
  return `header.${Buffer.from(JSON.stringify({ sub: "oauth:google:one", provider: "google", email_verified: true, ...extra })).toString("base64url")}.signature`;
}
function request(headers = {}, body = "{}", query = "") {
  return new NextRequest(`${origin}/api/customer-agent/connections/create${query}`, {
    method: "POST",
    headers: {
      cookie: `auth_token=${token()}; unrelated=private`,
      origin,
      "content-type": "application/json",
      ...headers,
    },
    body,
  });
}
const context = { params: Promise.resolve({ operation: "create" }) };

describe("customer connection browser boundary", () => {
  beforeEach(() => {
    vi.stubEnv("CUSTOMER_CONNECTIONS_ENABLED", "true");
    vi.stubEnv("NEXT_PUBLIC_APP_URL", origin);
    vi.stubEnv("QUERY_SERVICE_URL", "http://query.test");
  });
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });
  it("forwards only browser session and bounded JSON, never cookies or URL data", async () => {
    const fetcher = vi
      .fn()
      .mockResolvedValue(Response.json({ data: { token: "one-time-synthetic" } }));
    vi.stubGlobal("fetch", fetcher);
    const response = await POST(request(), context);
    expect(response.status).toBe(200);
    expect(response.headers.get("cache-control")).toBe("no-store");
    expect(String(fetcher.mock.calls[0]?.[0])).toBe(
      "http://query.test/api/customer-agent/connections/create"
    );
    expect(fetcher.mock.calls[0]?.[1]).toMatchObject({
      cache: "no-store",
      redirect: "error",
      body: "{}",
      headers: { authorization: `Bearer ${token()}`, "content-type": "application/json" },
    });
    expect(fetcher.mock.calls[0]?.[1].headers.cookie).toBeUndefined();
  });
  it.each([
    { origin: "https://evil.test" },
    { origin: "" },
    { "sec-fetch-site": "cross-site" },
    { authorization: "Bearer agent" },
    { cookie: "" },
    { cookie: "auth_token=header.bnVsbA.signature" },
    ...["is_api_key", "view_as", "is_demo"].map((key) => ({
      cookie: `auth_token=${token({ [key]: true })}`,
    })),
    { cookie: `auth_token=${token({ email_verified: false })}` },
    { cookie: `auth_token=${token({ core_api_key: "secret" })}` },
  ])("denies invalid browser authority %j", async (headers) => {
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    expect([401, 403]).toContain((await POST(request(headers), context)).status);
    expect(fetcher).not.toHaveBeenCalled();
  });
  it("bounds request, rejects queries and fails closed when disabled", async () => {
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    expect((await POST(request({}, "x".repeat(4097)), context)).status).toBe(413);
    expect((await POST(request({}, "{}", "?token=private"), context)).status).toBe(403);
    vi.stubEnv("CUSTOMER_CONNECTIONS_ENABLED", "false");
    expect((await POST(request(), context)).status).toBe(404);
    expect(fetcher).not.toHaveBeenCalled();
  });
  it("generic proxies cannot bypass the dedicated CSRF boundary", async () => {
    const fetcher = vi.fn();
    vi.stubGlobal("fetch", fetcher);
    expect(
      (
        await genericPost(request(), {
          params: Promise.resolve({ path: ["customer-agent", "connections", "create"] }),
        })
      ).status
    ).toBe(404);
    expect(
      (
        await v1Post(request(), {
          params: Promise.resolve({ path: ["..", "customer-agent", "connections", "create"] }),
        })
      ).status
    ).toBe(404);
    expect(fetcher).not.toHaveBeenCalled();
  });
  it("bounds upstream response and strips raw failure details", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response("x".repeat(65537))));
    expect((await POST(request(), context)).status).toBe(502);
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          Response.json(
            { error: { code: "access_denied", message: "private raw detail" } },
            { status: 403 }
          )
        )
    );
    const response = await POST(request(), context);
    expect(response.status).toBe(403);
    expect(await response.text()).not.toContain("private raw detail");
  });
});
