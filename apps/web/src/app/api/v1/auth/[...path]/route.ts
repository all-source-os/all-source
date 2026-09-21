import { type NextRequest, NextResponse } from "next/server";
import { publicUrl } from "@/lib/public-url";

/**
 * Runtime proxy for auth requests.
 *
 * When AUTH_SERVICE_URL is set, routes go to the better-auth-rs service
 * at /api/auth/{path}. Otherwise, falls back to the Control Plane (Go)
 * at /api/v1/auth/{path} — note the different path prefix.
 */

function getAuthBackend(path: string): { url: string; pathPrefix: string } {
  // CP owns product JWTs and tenant provisioning. The standalone auth service
  // owns durable credentials, not the browser's product-session contract.
  if (["register", "login", "session"].includes(path)) {
    return {
      url: process.env.CONTROL_PLANE_INTERNAL_URL || "http://localhost:3901",
      pathPrefix: "/api/v1/auth",
    };
  }
  if (["me", "logout"].includes(path)) {
    return {
      url: process.env.QUERY_SERVICE_URL || "http://localhost:3902",
      pathPrefix: "/api/v1/auth",
    };
  }
  const authService = process.env.AUTH_SERVICE_URL;
  if (authService) {
    return { url: authService, pathPrefix: "/api/auth" };
  }
  const controlPlane = process.env.CONTROL_PLANE_INTERNAL_URL || "http://localhost:3901";
  return { url: controlPlane, pathPrefix: "/api/v1/auth" };
}

async function proxyToAuthService(request: NextRequest, path: string): Promise<NextResponse> {
  if (request.method === "POST" && ["register", "login"].includes(path)) {
    const origin = request.headers.get("origin");
    if (origin && origin !== publicUrl(request, "/").origin) {
      return NextResponse.json(
        { message: "Cross-origin authentication is not allowed" },
        { status: 403 }
      );
    }
    if (!request.headers.get("content-type")?.toLowerCase().startsWith("application/json")) {
      return NextResponse.json({ message: "JSON request required" }, { status: 415 });
    }
  }
  const backend = getAuthBackend(path);
  const url = new URL(`${backend.pathPrefix}/${path}`, backend.url);

  request.nextUrl.searchParams.forEach((value, key) => {
    url.searchParams.set(key, value);
  });

  const headers: Record<string, string> = {
    "content-type": request.headers.get("content-type") || "application/json",
  };

  const cookie = request.headers.get("cookie");
  if (cookie) {
    headers.cookie = cookie;
  }
  const token = request.cookies.get("auth_token")?.value;
  const authorization = request.headers.get("authorization");
  if (authorization || token) headers.authorization = authorization || `Bearer ${token}`;

  const fetchOptions: RequestInit = {
    method: request.method,
    headers,
    signal: AbortSignal.timeout(20_000),
    redirect: "manual",
    cache: "no-store",
  };

  if (["POST", "PUT", "PATCH"].includes(request.method)) {
    fetchOptions.body = await request.text();
  }

  try {
    const response = await fetch(url.toString(), fetchOptions);
    const body = await response.text();

    if (request.method === "POST" && ["register", "login"].includes(path) && response.ok) {
      const data = JSON.parse(body);
      if (typeof data.token !== "string" || !data.token) {
        return NextResponse.json({ message: "Invalid authentication response" }, { status: 502 });
      }
      // Set the HttpOnly cookie here, never put a bearer token in a URL or
      // expose it to frontend JavaScript during email authentication.
      const sessionResponse = NextResponse.json(
        { session_established: true, new_user: data.new_user === true },
        { status: response.status, headers: { "cache-control": "no-store" } }
      );
      sessionResponse.cookies.set("auth_token", data.token, {
        httpOnly: true,
        secure: process.env.NODE_ENV === "production",
        sameSite: "lax",
        maxAge: 60 * 60 * 24 * 7,
        path: "/",
      });
      return sessionResponse;
    }

    const responseHeaders: Record<string, string> = {
      "content-type": response.headers.get("content-type") || "application/json",
    };
    const setCookie = response.headers.get("set-cookie");
    if (setCookie) {
      responseHeaders["set-cookie"] = setCookie;
    }

    return new NextResponse(body, {
      status: response.status,
      headers: responseHeaders,
    });
  } catch (_error) {
    return NextResponse.json(
      { message: "Authentication service is temporarily unavailable" },
      { status: 502 }
    );
  }
}

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToAuthService(request, path.join("/"));
}

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToAuthService(request, path.join("/"));
}

export async function PUT(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToAuthService(request, path.join("/"));
}

export async function DELETE(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToAuthService(request, path.join("/"));
}
