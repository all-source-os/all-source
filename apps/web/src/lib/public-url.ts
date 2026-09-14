import type { NextRequest } from "next/server";

/**
 * The origin the browser actually used to reach this app.
 *
 * `request.url` is not it, and that is the whole reason this module exists.
 * Next's standalone server rebuilds `request.url` from its own bind address, so
 * behind Fly's proxy it reads `http://0.0.0.0:3000` (PORT and HOSTNAME in
 * fly.toml). Any redirect built with `new URL(path, request.url)` therefore
 * sends the visitor to a port on their own machine.
 *
 * Every redirect in this app goes through here. Building one from `request.url`
 * reintroduces the bug on whichever route forgets.
 *
 * NEXT_PUBLIC_APP_URL wins over the forwarded headers deliberately: it is set in
 * fly.toml, and a configured origin cannot be steered by a request header.
 */
export function publicOrigin(request: NextRequest): string {
  const configured = process.env.NEXT_PUBLIC_APP_URL;
  if (configured) {
    return configured.replace(/\/+$/, "");
  }

  const host = request.headers.get("x-forwarded-host") ?? request.headers.get("host");
  if (host) {
    const proto = request.headers.get("x-forwarded-proto") ?? "https";
    return `${proto}://${host}`;
  }

  return new URL(request.url).origin;
}

/**
 * Absolute URL for an in-app `path`, on the origin the browser can reach.
 *
 * `path` must already be same-origin — see `safeNextPath` in the auth callback,
 * which rejects `//host` and absolute URLs before they get here.
 */
export function publicUrl(request: NextRequest, path: string): URL {
  return new URL(path, publicOrigin(request));
}
