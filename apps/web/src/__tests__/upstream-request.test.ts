import { describe, expect, it } from "vitest";
import { clientIpForUpstream, readCappedText } from "@/lib/upstream-request";

describe("clientIpForUpstream", () => {
  it("prefers Fly-Client-IP over any forwarded-for entry", () => {
    const headers = new Headers({
      "fly-client-ip": "198.51.100.7",
      "x-forwarded-for": "203.0.113.5, 198.51.100.7",
    });
    expect(clientIpForUpstream(headers)).toBe("198.51.100.7");
  });

  it("ignores client-supplied forwarded-for entries and uses the proxy's append", () => {
    const headers = new Headers({ "x-forwarded-for": "203.0.113.5, 192.0.2.44" });
    expect(clientIpForUpstream(headers)).toBe("192.0.2.44");
  });

  it("returns undefined when no address header is present", () => {
    expect(clientIpForUpstream(new Headers())).toBeUndefined();
  });
});

function streamedRequest(parts: string[], headers: Record<string, string> = {}): Request {
  const encoder = new TextEncoder();
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const part of parts) controller.enqueue(encoder.encode(part));
      controller.close();
    },
  });
  return new Request("http://localhost/", {
    method: "POST",
    body,
    headers,
    duplex: "half",
  } as RequestInit);
}

describe("readCappedText", () => {
  it("returns the body when it fits", async () => {
    expect(await readCappedText(streamedRequest(["ab", "cd"]), 4)).toBe("abcd");
  });

  it("stops reading a stream with no content-length once it passes the cap", async () => {
    expect(await readCappedText(streamedRequest(["abc", "de"]), 4)).toBeNull();
  });

  it("rejects on a declared content-length above the cap without reading", async () => {
    const request = streamedRequest(["a"], { "content-length": "5000" });
    expect(await readCappedText(request, 4096)).toBeNull();
  });
});
