import { type NextRequest, NextResponse } from "next/server";
import { carriesLegacyApiKey, decodeJwtPayload } from "@/lib/server/session-token";

const noStore = { "Cache-Control": "no-store" };
function failure(status: number, message: string) {
  return NextResponse.json(
    { error: { code: "team_request_failed", message } },
    { status, headers: noStore }
  );
}

async function limitedText(
  stream: ReadableStream<Uint8Array> | null,
  limit: number,
  signal: AbortSignal
) {
  if (!stream) return "";
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  const abort = () => {
    void reader.cancel().catch(() => {});
  };
  signal.addEventListener("abort", abort, { once: true });
  try {
    while (true) {
      signal.throwIfAborted();
      const { done, value } = await reader.read();
      signal.throwIfAborted();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > limit) throw new Error("body limit");
      chunks.push(value);
    }
    return Buffer.concat(chunks).toString("utf8");
  } finally {
    signal.removeEventListener("abort", abort);
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

function targetPath(method: string, path: string[]): string | null {
  if (
    path.length === 1 &&
    ((method === "GET" && ["members", "agent-keys"].includes(path[0] ?? "")) ||
      (method === "POST" && ["invite", "join", "agent-keys"].includes(path[0] ?? "")))
  )
    return path[0] ?? null;
  const id = path[1];
  if (!id || id.length > 200 || !/^[A-Za-z0-9:_ .@-]+$/.test(id) || [".", ".."].includes(id))
    return null;
  if (
    path[0] === "members" &&
    ((method === "DELETE" && path.length === 2) ||
      (method === "PUT" && path.length === 3 && path[2] === "role"))
  )
    return `members/${encodeURIComponent(id)}`;
  if (path[0] === "agent-keys" && method === "DELETE" && path.length === 2)
    return `agent-keys/${encodeURIComponent(id)}`;
  return null;
}

async function proxy(request: NextRequest, path: string[]) {
  const target = targetPath(request.method, path);
  if (!target) return failure(404, "Team route not found.");
  const token = request.cookies.get("auth_token")?.value;
  // This route serves browser sessions. SDKs use Control Plane directly.
  if (!token || request.headers.has("authorization"))
    return failure(401, "Sign in again to manage your workspace.");
  const claims = decodeJwtPayload(token);
  if (
    !claims ||
    typeof claims !== "object" ||
    Array.isArray(claims) ||
    typeof claims.sub !== "string" ||
    carriesLegacyApiKey(token)
  )
    return failure(401, "Sign in again to manage your workspace.");
  if (claims.is_api_key || claims.view_as || claims.is_demo)
    return failure(403, "A personal workspace session is required.");
  if (request.method !== "GET") {
    const configured = process.env.NEXT_PUBLIC_APP_URL;
    const expected = configured
      ? new URL(configured).origin
      : process.env.NODE_ENV === "production"
        ? null
        : request.nextUrl.origin;
    if (
      !expected ||
      request.headers.get("origin") !== expected ||
      request.headers.get("sec-fetch-site") === "cross-site"
    )
      return failure(403, "Open this action from your workspace.");
  }
  let body: string | undefined;
  if (["POST", "PUT"].includes(request.method)) {
    if (request.headers.get("content-type")?.split(";")[0]?.trim() !== "application/json")
      return failure(415, "JSON is required.");
    try {
      body = await limitedText(request.body, 4096, AbortSignal.timeout(10_000));
    } catch {
      return failure(413, "Request is too large or incomplete.");
    }
  }
  try {
    const deadline = AbortSignal.timeout(35_000);
    const response = await fetch(
      new URL(
        `/api/v1/teams/${target}`,
        process.env.CONTROL_PLANE_INTERNAL_URL || "http://localhost:3903"
      ).toString(),
      {
        method: request.method,
        headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
        body,
        cache: "no-store",
        redirect: "error",
        signal: deadline,
      }
    );
    if (response.status === 204) return new NextResponse(null, { status: 204, headers: noStore });
    const text = await limitedText(response.body, 262_144, deadline);
    const data = JSON.parse(text);
    if (target === "join" && response.ok) {
      if (typeof data.token !== "string" || carriesLegacyApiKey(data.token))
        return failure(502, "Workspace session was not established. Retry the invitation code.");
      const next = decodeJwtPayload(data.token);
      if (
        next.sub !== claims.sub ||
        next.tenant_id !== data.tenant_id ||
        typeof next.exp !== "number" ||
        next.exp <= Date.now() / 1000 ||
        next.is_api_key ||
        next.view_as ||
        next.is_demo
      )
        return failure(502, "Invalid workspace session.");
      const result = NextResponse.json(
        { joined: true, tenant_id: data.tenant_id },
        { headers: noStore }
      );
      result.cookies.set("auth_token", data.token, {
        httpOnly: true,
        secure: process.env.NODE_ENV === "production",
        sameSite: "lax",
        path: "/",
        expires: new Date(next.exp * 1000),
      });
      return result;
    }
    if (!response.ok) {
      const message = data.error?.message || data.message;
      return failure(
        response.status,
        typeof message === "string" ? message : "Team request failed."
      );
    }
    return NextResponse.json(data, { status: response.status, headers: noStore });
  } catch {
    return failure(502, "Team service is unavailable. Retry the operation.");
  }
}

type Context = { params: Promise<{ path: string[] }> };
export async function GET(request: NextRequest, { params }: Context) {
  return proxy(request, (await params).path);
}
export async function POST(request: NextRequest, { params }: Context) {
  return proxy(request, (await params).path);
}
export async function PUT(request: NextRequest, { params }: Context) {
  return proxy(request, (await params).path);
}
export async function DELETE(request: NextRequest, { params }: Context) {
  return proxy(request, (await params).path);
}
