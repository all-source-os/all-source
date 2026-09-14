import { afterEach, describe, expect, test } from "bun:test";
import { NextRequest } from "next/server";

import { publicOrigin, publicUrl } from "./public-url";

// The deployed admin sent operators to http://0.0.0.0:3001/tenants after login.
// Next's standalone server builds `request.url` from PORT and HOSTNAME, so a
// request that arrived at admin.all-source.xyz is presented to route handlers
// as one that arrived at the bind address. These fixtures reproduce that.
const BIND_ADDRESS_URL = "http://0.0.0.0:3001/api/auth/callback?token=abc";

function requestAsSeenByNext(headers: Record<string, string> = {}) {
  return new NextRequest(BIND_ADDRESS_URL, { headers });
}

const originalAppUrl = process.env.NEXT_PUBLIC_APP_URL;

afterEach(() => {
  if (originalAppUrl === undefined) {
    delete process.env.NEXT_PUBLIC_APP_URL;
  } else {
    process.env.NEXT_PUBLIC_APP_URL = originalAppUrl;
  }
});

describe("publicOrigin", () => {
  test("prefers the configured origin over the bind address", () => {
    process.env.NEXT_PUBLIC_APP_URL = "https://admin.all-source.xyz";

    expect(publicOrigin(requestAsSeenByNext())).toBe("https://admin.all-source.xyz");
  });

  test("ignores a forwarded host when an origin is configured", () => {
    process.env.NEXT_PUBLIC_APP_URL = "https://admin.all-source.xyz";

    const origin = publicOrigin(requestAsSeenByNext({ "x-forwarded-host": "evil.example" }));

    expect(origin).toBe("https://admin.all-source.xyz");
  });

  test("trims a trailing slash so paths do not double up", () => {
    process.env.NEXT_PUBLIC_APP_URL = "https://admin.all-source.xyz/";

    expect(publicUrl(requestAsSeenByNext(), "/tenants").toString()).toBe(
      "https://admin.all-source.xyz/tenants"
    );
  });

  test("falls back to the forwarded headers when nothing is configured", () => {
    delete process.env.NEXT_PUBLIC_APP_URL;

    const origin = publicOrigin(
      requestAsSeenByNext({
        "x-forwarded-host": "admin.all-source.xyz",
        "x-forwarded-proto": "https",
      })
    );

    expect(origin).toBe("https://admin.all-source.xyz");
  });
});

describe("publicUrl", () => {
  test("never returns the bind address the incident reported", () => {
    process.env.NEXT_PUBLIC_APP_URL = "https://admin.all-source.xyz";

    const url = publicUrl(requestAsSeenByNext(), "/tenants");

    expect(url.toString()).toBe("https://admin.all-source.xyz/tenants");
    expect(url.host).not.toBe("0.0.0.0:3001");
  });

  test("keeps query parameters the caller adds", () => {
    process.env.NEXT_PUBLIC_APP_URL = "https://admin.all-source.xyz";

    const url = publicUrl(requestAsSeenByNext(), "/login");
    url.searchParams.set("error", "invalid_token");

    expect(url.toString()).toBe("https://admin.all-source.xyz/login?error=invalid_token");
  });
});
