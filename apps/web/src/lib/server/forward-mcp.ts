import { type NextRequest, NextResponse } from "next/server";
import { boundedText, privateHeaders } from "@/lib/server/customer-agent-http";

function failure(status: number, error: string, headers = {}) {
  return NextResponse.json({ error }, { status, headers: { ...privateHeaders, ...headers } });
}

function upstream(base: string | undefined, path: string) {
  try {
    const url = new URL(base || "");
    if (url.username || url.password || url.search || url.hash || url.pathname !== "/") return null;
    if (
      url.protocol !== "https:" &&
      !(url.protocol === "http:" && ["127.0.0.1", "[::1]"].includes(url.hostname))
    )
      return null;
    return new URL(path, url);
  } catch {
    return null;
  }
}

export async function forwardMcp(
  request: NextRequest,
  profile: {
    enabled: boolean;
    origin: string | null;
    upstream: string | undefined;
    path: string;
    origins: string | undefined;
    metadata: string;
    scope: string;
    sessions?: boolean;
  }
) {
  if (!profile.enabled) return failure(404, "unavailable");
  const issuer = profile.origin;
  const target = upstream(profile.upstream, profile.path);
  if (!issuer || !target) return failure(503, "unavailable");
  if (request.nextUrl.search) return failure(403, "access_denied");
  const origin = request.headers.get("origin");
  const allowedOrigins = (profile.origins || "").split(",").filter(Boolean);
  if (origin !== null && !allowedOrigins.includes(origin)) return failure(403, "access_denied");
  const credential = request.headers.get("authorization");
  if (!credential?.startsWith("Bearer ") || credential.length <= 7 || credential.length > 3807) {
    return failure(401, "invalid_token", {
      "WWW-Authenticate": `Bearer resource_metadata="${issuer}${profile.metadata}", scope="${profile.scope}"`,
    });
  }
  const deadline = AbortSignal.timeout(70_000);
  const headers = new Headers({ authorization: credential });
  for (const name of [
    "accept",
    "content-type",
    "origin",
    "mcp-protocol-version",
    ...(profile.sessions ? ["mcp-session-id"] : []),
  ]) {
    const value = request.headers.get(name);
    if (value) headers.set(name, value);
  }
  let body: string | undefined;
  if (request.method === "POST") {
    try {
      body = await boundedText(request.body, 65_536, AbortSignal.timeout(5_000));
    } catch {
      return failure(413, "input_too_large");
    }
  }
  try {
    const response = await fetch(target, {
      method: request.method,
      headers,
      body,
      cache: "no-store",
      redirect: "error",
      signal: deadline,
    });
    const text = await boundedText(response.body, 65_536, deadline);
    const responseHeaders = new Headers(privateHeaders);
    responseHeaders.set("Content-Type", "application/json");
    for (const name of [
      "www-authenticate",
      "allow",
      "retry-after",
      ...(profile.sessions ? ["mcp-session-id"] : []),
    ]) {
      const value = response.headers.get(name);
      if (value) responseHeaders.set(name, value);
    }
    if (response.status === 202 && text === "")
      return new NextResponse(null, { status: 202, headers: responseHeaders });
    // Only protocol JSON crosses the boundary; upstream HTML/errors stay private.
    return NextResponse.json(JSON.parse(text), {
      status: response.status,
      headers: responseHeaders,
    });
  } catch {
    return failure(503, "unavailable");
  }
}
