import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * `new URL(path, request.url)` is how the deployed admin sent operators to
 * http://0.0.0.0:3001/tenants after login: Next's standalone server rebuilds
 * `request.url` from PORT and HOSTNAME, so behind Fly's proxy it is the bind
 * address, not the browser's origin.
 *
 * Fixing the six call sites does not stop a seventh being written. This does.
 * Route redirects go through `publicUrl` (lib/public-url.ts); the helper itself
 * is the one place allowed to read `request.url`, as a last-resort fallback.
 */
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

describe("redirect origin", () => {
  test("no route builds a URL from request.url", () => {
    const offenders = sourceFiles(SOURCE_ROOT)
      .filter((path) => path !== HELPER)
      .filter((path) => REQUEST_URL_BASE.test(readFileSync(path, "utf8")))
      .map((path) => path.slice(SOURCE_ROOT.length + 1));

    expect(offenders).toEqual([]);
  });

  test("the scan can see a violation", () => {
    expect(REQUEST_URL_BASE.test('new URL("/tenants", request.url)')).toBe(true);
    expect(REQUEST_URL_BASE.test('publicUrl(request, "/tenants")')).toBe(false);
  });
});
