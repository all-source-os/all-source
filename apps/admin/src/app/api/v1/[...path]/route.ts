import { type NextRequest, NextResponse } from "next/server";
import { getControlPlaneUrl } from "@/lib/auth";

/**
 * Backend-For-Frontend data proxy for Control Plane admin requests.
 *
 * The admin app is served from admin.all-source.xyz while the Control
 * Plane lives at a different origin (api.all-source.xyz). The CP authenticates
 * with `Authorization: Bearer <jwt>` ONLY — it ignores cookies. The admin JWT
 * is stored in an httpOnly `admin_token` cookie that browser JS cannot read and
 * that is not sent cross-origin, so a direct cross-origin fetch from the client
 * is always unauthenticated (401).
 *
 * This server-side catch-all runs on the admin origin, reads the httpOnly
 * cookie, and forwards the request to the Control Plane with the Bearer token
 * attached. Client API calls hit `/api/v1/...` same-origin and land here.
 *
 * The more specific nested route `/api/v1/auth/[...path]` (and
 * `/api/v1/auth/oauth/[...path]`) takes precedence for auth flows; this
 * catch-all serves the data routes: `/api/v1/admin/*`, `/api/v1/policies`,
 * `/api/v1/cluster/*`, etc.
 *
 * Mirrors the structure of `src/app/api/v1/auth/[...path]/route.ts`. Cookies are
 * NOT forwarded to the CP (auth is via Bearer, not cookie).
 */

async function proxyToControlPlane(request: NextRequest, path: string): Promise<NextResponse> {
  const url = new URL(`/api/v1/${path}`, getControlPlaneUrl());

  request.nextUrl.searchParams.forEach((value, key) => {
    url.searchParams.set(key, value);
  });

  const headers: Record<string, string> = {
    "content-type": request.headers.get("content-type") || "application/json",
  };

  const token = request.cookies.get("admin_token")?.value;
  if (token) {
    headers.authorization = `Bearer ${token}`;
  }

  const fetchOptions: RequestInit = {
    method: request.method,
    headers,
    cache: "no-store",
  };

  if (["POST", "PUT", "PATCH"].includes(request.method)) {
    fetchOptions.body = await request.text();
  }

  try {
    const response = await fetch(url.toString(), fetchOptions);
    const body = await response.text();

    return new NextResponse(body, {
      status: response.status,
      headers: {
        "cache-control": "private, no-store",
        "content-type": response.headers.get("content-type") || "application/json",
      },
    });
  } catch {
    return NextResponse.json({ error: "Failed to reach Control Plane" }, { status: 502 });
  }
}

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToControlPlane(request, path.join("/"));
}

export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToControlPlane(request, path.join("/"));
}

export async function PUT(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToControlPlane(request, path.join("/"));
}

export async function PATCH(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToControlPlane(request, path.join("/"));
}

export async function DELETE(
  request: NextRequest,
  { params }: { params: Promise<{ path: string[] }> }
) {
  const { path } = await params;
  return proxyToControlPlane(request, path.join("/"));
}
