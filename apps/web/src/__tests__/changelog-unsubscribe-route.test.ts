import { NextRequest } from "next/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { POST } from "@/app/api/changelog/unsubscribe/route";

const id = "0123456789abcdef0123456789abcdef";
const token = "a".repeat(64);

function unsubscribeRequest(body: string): NextRequest {
  return new NextRequest("http://localhost/api/changelog/unsubscribe", {
    method: "POST",
    headers: { "content-type": "application/json", "fly-client-ip": "198.51.100.7" },
    body,
  });
}

describe("POST /api/changelog/unsubscribe", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.unstubAllEnvs();
  });

  it("forwards only the id and token to Control Plane", async () => {
    vi.stubEnv("CONTROL_PLANE_INTERNAL_URL", "https://control.example.test");
    const fetchMock = vi
      .fn()
      .mockResolvedValue(Response.json({ status: "unsubscribed" }, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const response = await POST(unsubscribeRequest(JSON.stringify({ id, token, extra: "x" })));

    expect(response.status).toBe(200);
    const [url, init] = fetchMock.mock.calls[0] ?? [];
    expect(url).toBe("https://control.example.test/api/v1/changelog/unsubscribe");
    expect(init.headers).toMatchObject({ "x-forwarded-for": "198.51.100.7" });
    expect(JSON.parse(init.body)).toEqual({ id, token });
  });

  it.each([
    ["malformed JSON", "{"],
    ["short id", JSON.stringify({ id: "abc", token })],
    ["non-hex token", JSON.stringify({ id, token: "z".repeat(64) })],
  ])("rejects %s without forwarding", async (_, body) => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    const response = await POST(unsubscribeRequest(body));

    expect(response.status).toBe(400);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("returns a generic 503 when Control Plane is unavailable", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("private upstream detail")));

    const response = await POST(unsubscribeRequest(JSON.stringify({ id, token })));

    expect(response.status).toBe(503);
    await expect(response.json()).resolves.toMatchObject({ error: "unsubscribe_unavailable" });
  });
});
