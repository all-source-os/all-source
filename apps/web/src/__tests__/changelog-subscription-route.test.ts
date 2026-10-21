import { NextRequest } from "next/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { POST } from "@/app/api/changelog/subscriptions/route";

function subscriptionRequest(body: string): NextRequest {
  return new NextRequest("http://localhost/api/changelog/subscriptions", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "fly-client-ip": "198.51.100.7",
      "x-forwarded-for": "203.0.113.5, 198.51.100.7",
    },
    body,
  });
}

const valid = { email: " Ada@Example.com ", consent: true, placement: "home" };

describe("POST /api/changelog/subscriptions", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
  });

  it("forwards a normalised signup to Control Plane with the edge-reported client IP", async () => {
    vi.stubEnv("CONTROL_PLANE_INTERNAL_URL", "https://control.example.test");
    const fetchMock = vi
      .fn()
      .mockResolvedValue(Response.json({ status: "subscribed", message: "ok" }, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const response = await POST(
      subscriptionRequest(
        JSON.stringify({
          ...valid,
          campaign_source: { source: "github", medium: "community", extra: "dropped" },
          cf_turnstile_response: "token",
          unexpected: "dropped",
        })
      )
    );

    expect(response.status).toBe(200);
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe("https://control.example.test/api/v1/changelog/subscriptions");
    expect(init.headers).toMatchObject({ "x-forwarded-for": "198.51.100.7" });
    expect(init.signal).toBeInstanceOf(AbortSignal);
    expect(JSON.parse(init.body)).toEqual({
      email: "ada@example.com",
      consent: true,
      placement: "home",
      campaign_source: { source: "github", medium: "community" },
      cf_turnstile_response: "token",
    });
  });

  it.each([
    ["missing email", { ...valid, email: "" }],
    ["malformed email", { ...valid, email: "ada@" }],
    ["display-name email", { ...valid, email: "Ada <ada@example.com>" }],
    ["no consent", { ...valid, consent: false }],
    ["unknown placement", { ...valid, placement: "footer" }],
  ])("rejects %s without forwarding", async (_, body) => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    const response = await POST(subscriptionRequest(JSON.stringify(body)));

    expect(response.status).toBe(400);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("rejects malformed JSON and oversized bodies before forwarding", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    expect((await POST(subscriptionRequest("{"))).status).toBe(400);
    expect((await POST(subscriptionRequest("x".repeat(4 * 1024 + 1)))).status).toBe(413);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("passes the rate-limit status and retry-after through", async () => {
    vi.stubGlobal(
      "fetch",
      vi
        .fn()
        .mockResolvedValue(
          Response.json(
            { error: "rate_limited", retry_after: 120 },
            { status: 429, headers: { "retry-after": "120" } }
          )
        )
    );

    const response = await POST(subscriptionRequest(JSON.stringify(valid)));

    expect(response.status).toBe(429);
    expect(response.headers.get("retry-after")).toBe("120");
  });

  it("returns a generic 503 and logs nothing when Control Plane is unavailable", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("private upstream detail")));
    const consoleSpies = (["log", "info", "warn", "error"] as const).map((level) =>
      vi.spyOn(console, level).mockImplementation(() => {})
    );

    const response = await POST(subscriptionRequest(JSON.stringify(valid)));

    expect(response.status).toBe(503);
    await expect(response.json()).resolves.toEqual({
      error: "subscription_unavailable",
      message: "Signups are temporarily unavailable. Please try again later.",
    });
    for (const spy of consoleSpies) expect(spy).not.toHaveBeenCalled();
  });
});
