import { type NextRequest, NextResponse } from "next/server";
import {
  boundedText,
  claudeCallback,
  connectionOrigin,
  privateHeaders,
  productSession,
  queryConnection,
  remoteEnabled,
  remoteIssuer,
  remoteRequestCookie,
  sameOrigin,
} from "@/lib/server/customer-agent-http";

function failure(status: number, error: string) {
  return NextResponse.json({ error }, { status, headers: privateHeaders });
}
function clearRequest(response: NextResponse) {
  response.cookies.set(remoteRequestCookie, "", {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    maxAge: 0,
  });
  return response;
}
function view(request: NextRequest, error?: string) {
  const origin = connectionOrigin(request);
  if (!origin) return failure(503, "temporarily_unavailable");
  const url = new URL("/connect/claude", origin);
  if (error) url.searchParams.set("error", error);
  return NextResponse.redirect(url, { status: 303, headers: privateHeaders });
}

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ action: string }> }
) {
  const { action } = await params;
  if (!remoteEnabled() || action !== "authorize") return failure(404, "unavailable");
  if (!connectionOrigin(request) || !remoteIssuer()) return failure(503, "temporarily_unavailable");
  if (request.headers.has("authorization") || request.nextUrl.search.length > 4096)
    return failure(400, "invalid_request");
  const entries = [...request.nextUrl.searchParams.entries()];
  if (new Set(entries.map(([key]) => key)).size !== entries.length)
    return failure(400, "invalid_request");
  try {
    const result = await queryConnection(
      "oauth/prepare",
      JSON.stringify(Object.fromEntries(entries))
    );
    if (
      !result.ok ||
      typeof result.data.request_token !== "string" ||
      result.data.request_token.length > 3800
    )
      return failure(result.ok ? 502 : result.status, "invalid_request");
    const response = view(request);
    response.cookies.set(remoteRequestCookie, result.data.request_token, {
      httpOnly: true,
      secure: process.env.NODE_ENV === "production",
      sameSite: "lax",
      path: "/",
      maxAge: 600,
    });
    return response;
  } catch {
    return failure(503, "temporarily_unavailable");
  }
}

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ action: string }> }
) {
  const { action } = await params;
  if (!remoteEnabled() || !["token", "decision"].includes(action))
    return failure(404, "unavailable");
  if (request.nextUrl.search || request.headers.has("authorization"))
    return failure(400, "invalid_request");
  if (
    request.headers.get("content-type")?.split(";")[0]?.trim() !==
    "application/x-www-form-urlencoded"
  )
    return failure(415, "invalid_request");
  try {
    const body = await boundedText(
      request.body,
      action === "token" ? 8192 : 512,
      AbortSignal.timeout(5_000)
    );
    if (action === "token") {
      const result = await queryConnection("oauth/token", body, undefined, true);
      return NextResponse.json(result.data, { status: result.status, headers: privateHeaders });
    }
    if (!sameOrigin(request)) return failure(403, "access_denied");
    return await decision(request, body);
  } catch {
    return failure(503, "temporarily_unavailable");
  }
}

async function decision(request: NextRequest, body: string) {
  const fields = new URLSearchParams(body);
  const entries = [...fields.entries()];
  if (
    new Set(entries.map(([key]) => key)).size !== entries.length ||
    entries.some(([key]) => !["decision", "consent"].includes(key))
  )
    return failure(400, "invalid_request");
  const requestToken = request.cookies.get(remoteRequestCookie)?.value;
  if (!requestToken || requestToken.length > 3800) return view(request, "expired");
  const inspected = await queryConnection(
    "oauth/inspect",
    JSON.stringify({ request_token: requestToken })
  );
  const issuer = remoteIssuer();
  if (
    !issuer ||
    !inspected.ok ||
    inspected.data.redirect_uri !== claudeCallback ||
    inspected.data.issuer !== issuer ||
    typeof inspected.data.state !== "string"
  )
    return clearRequest(view(request, "expired"));
  if (fields.get("decision") === "signin") {
    const origin = connectionOrigin(request);
    if (!origin) return failure(503, "temporarily_unavailable");
    const login = new URL("/login?next=%2Fconnect%2Fclaude", origin);
    const response = NextResponse.redirect(login, { status: 303, headers: privateHeaders });
    response.cookies.set("auth_token", "", {
      httpOnly: true,
      secure: process.env.NODE_ENV === "production",
      sameSite: "lax",
      path: "/",
      maxAge: 0,
    });
    return response;
  }
  const callback = new URL(claudeCallback);
  callback.searchParams.set("state", inspected.data.state);
  callback.searchParams.set("iss", inspected.data.issuer);
  if (fields.get("decision") === "deny") {
    callback.searchParams.set("error", "access_denied");
    return clearRequest(NextResponse.redirect(callback, { status: 303, headers: privateHeaders }));
  }
  const session = request.cookies.get("auth_token")?.value;
  if (
    fields.get("decision") !== "allow" ||
    fields.get("consent") !== "yes" ||
    !productSession(session)
  )
    return view(request, "denied");
  const authorized = await queryConnection(
    "connections/authorize",
    JSON.stringify({
      request_token: requestToken,
      consent: { accepted: true, version: "review-metadata-v1" },
    }),
    session
  );
  if (!authorized.ok) return view(request, authorized.status === 403 ? "denied" : "unavailable");
  if (
    authorized.data.redirect_uri !== claudeCallback ||
    authorized.data.issuer !== issuer ||
    authorized.data.state !== inspected.data.state ||
    typeof authorized.data.code !== "string" ||
    authorized.data.code.length > 4096
  )
    return view(request, "unavailable");
  callback.searchParams.set("code", authorized.data.code);
  return clearRequest(NextResponse.redirect(callback, { status: 303, headers: privateHeaders }));
}
