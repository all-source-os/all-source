import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { NextRequest } from "next/server";
import { afterEach, describe, expect, it } from "vitest";

import { publicOrigin, publicUrl } from "@/lib/public-url";

// Next's standalone server builds `request.url` from PORT and HOSTNAME, so a
// request that arrived at www.all-source.xyz is presented to route handlers as
// one that arrived at the bind address. The deployed admin shipped that bug and
// sent operators to http://0.0.0.0:3001 after login; web has the same shape.
const BIND_ADDRESS_URL = "http://0.0.0.0:3000/api/auth/callback?token=abc";

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
  it("prefers the configured origin over the bind address", () => {
    process.env.NEXT_PUBLIC_APP_URL = "https://www.all-source.xyz";

    expect(publicOrigin(requestAsSeenByNext())).toBe("https://www.all-source.xyz");
  });

  it("ignores a forwarded host when an origin is configured", () => {
    process.env.NEXT_PUBLIC_APP_URL = "https://www.all-source.xyz";

    const origin = publicOrigin(requestAsSeenByNext({ "x-forwarded-host": "evil.example" }));

    expect(origin).toBe("https://www.all-source.xyz");
  });

  it("falls back to the forwarded headers when nothing is configured", () => {
    delete process.env.NEXT_PUBLIC_APP_URL;

    const origin = publicOrigin(
      requestAsSeenByNext({
        "x-forwarded-host": "www.all-source.xyz",
        "x-forwarded-proto": "https",
      })
    );

    expect(origin).toBe("https://www.all-source.xyz");
  });
});

describe("publicUrl", () => {
  it("never returns the bind address", () => {
    process.env.NEXT_PUBLIC_APP_URL = "https://www.all-source.xyz";

    const url = publicUrl(requestAsSeenByNext(), "/dashboard");

    expect(url.toString()).toBe("https://www.all-source.xyz/dashboard");
    expect(url.host).not.toBe("0.0.0.0:3000");
  });

  it("trims a trailing slash so paths do not double up", () => {
    process.env.NEXT_PUBLIC_APP_URL = "https://www.all-source.xyz/";

    expect(publicUrl(requestAsSeenByNext(), "/onboarding").toString()).toBe(
      "https://www.all-source.xyz/onboarding"
    );
  });
});

/**
 * Fixing the call sites does not stop the next one being written, and this is
 * the defect that already shipped. `publicUrl` (lib/public-url.ts) is the only
 * place allowed to read `request.url`, as a last-resort fallback.
 */
describe("redirect origin", () => {
  const SOURCE_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
  const HELPER = join(SOURCE_ROOT, "lib", "public-url.ts");
  const REQUEST_URL_BASE = /new URL\((?![^)]*\bpublicOrigin\b)[^)]*\brequest\.url\b/;

  function sourceFiles(dir: string): string[] {
    return readdirSync(dir).flatMap((entry) => {
      const path = join(dir, entry);
      if (statSync(path).isDirectory()) {
        return sourceFiles(path);
      }
      return /\.tsx?$/.test(entry) && !/\.test\.tsx?$/.test(entry) ? [path] : [];
    });
  }

  it("no route builds a URL from request.url", () => {
    const offenders = sourceFiles(SOURCE_ROOT)
      .filter((path) => path !== HELPER)
      .filter((path) => REQUEST_URL_BASE.test(readFileSync(path, "utf8")))
      .map((path) => path.slice(SOURCE_ROOT.length + 1));

    expect(offenders).toEqual([]);
  });

  it("the scan can see a violation", () => {
    expect(REQUEST_URL_BASE.test('new URL("/dashboard", request.url)')).toBe(true);
    expect(REQUEST_URL_BASE.test('publicUrl(request, "/dashboard")')).toBe(false);
  });
});
