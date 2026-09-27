import { type NextRequest, NextResponse } from "next/server";

/**
 * Catch-all proxy for Query Service API requests.
 *
 * Routes all /api/* requests (that aren't handled by more specific routes like
 * /api/v1/*, /api/auth/*, /api/status/*) to the Query Service backend.
 *
 * This eliminates CORS issues by keeping all browser requests same-origin.
 *
 * Target the Query Service DIRECTLY, not NEXT_PUBLIC_API_URL. The latter points
 * at the branded gateway (api.all-source.xyz / Control Plane), which serves a
 * narrow slice of /api/v1/* (e.g. /api/v1/events/query, /api/v1/prime/graph)
 * but routes NONE of the non-v1 dashboard read API: /api/events, /api/streams,
 * /api/event-types, /api/tenant, /api/auth/me, /api/billing/* all return
 * "404 page not found" there, blanking every data view. The Query Service
 * serves the full non-v1 read API. Resolve it the same way /api/auth/session
 * does; override with QUERY_SERVICE_URL. Read at request time (not build time)
 * so it works on Vercel.
 */

function getQueryServiceUrl(): string {
  return (
    process.env.QUERY_SERVICE_URL ||
    (process.env.NODE_ENV === "production"
      ? "https://allsource-query.fly.dev"
      : "http://localhost:3902")
  );
}

async function buildProxyResponse(response: Response): Promise<NextResponse> {
  const body = await response.text();
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
}

async function proxyToQueryService(request: NextRequest, path: string): Promise<NextResponse> {
  const url = new URL(`/api/${path}`, getQueryServiceUrl());
  // Connection management belongs to the cookie/CSRF-protected specific route.
  if (url.pathname.startsWith("/api/customer-agent/connections")) {
    return NextResponse.json({ error: "Unsupported connection route" }, { status: 404 });
  }

  // Forward query params
  request.nextUrl.searchParams.forEach((value, key) => {
    url.searchParams.set(key, value);
  });

  const headers: Record<string, string> = {
    "content-type": request.headers.get("content-type") || "application/json",
  };

  // Forward auth: prefer explicit Authorization header, fall back to auth_token cookie
  const authorization = request.headers.get("authorization");
  if (authorization) {
    headers.authorization = authorization;
  } else {
    const authToken = request.cookies.get("auth_token")?.value;
    if (authToken) {
      headers.authorization = `Bearer ${authToken}`;
    }
  }
  const cookie = request.headers.get("cookie");
  if (cookie) {
    headers.cookie = cookie;
  }

  const fetchOptions: RequestInit = {
    method: request.method,
    headers,
  };

  // Forward body for POST/PUT/PATCH/DELETE
  if (["POST", "PUT", "PATCH", "DELETE"].includes(request.method) && request.body) {
    fetchOptions.body = await request.text();
  }

  try {
    // Use manual redirect so we can re-issue POST requests with body to the new location
    const response = await fetch(url.toString(), { ...fetchOptions, redirect: "manual" });

    // Follow redirects server-side, preserving method, body, and auth headers
    if (
      response.status === 301 ||
      response.status === 302 ||
      response.status === 307 ||
      response.status === 308
    ) {
      const location = response.headers.get("location");
      if (location) {
        const redirectResponse = await fetch(location, fetchOptions);
        return buildProxyResponse(redirectResponse);
      }
    }

    return buildProxyResponse(response);
  } catch (_error) {
    return NextResponse.json({ error: "Failed to reach Query Service" }, { status: 502 });
  }
}

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToQueryService(request, path.join("/"));
}

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToQueryService(request, path.join("/"));
}

export async function PUT(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToQueryService(request, path.join("/"));
}

export async function DELETE(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToQueryService(request, path.join("/"));
}

export async function PATCH(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToQueryService(request, path.join("/"));
}
