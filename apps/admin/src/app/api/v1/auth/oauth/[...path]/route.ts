import { type NextRequest, NextResponse } from "next/server";
import { getControlPlaneUrl } from "@/lib/auth";
import { publicOrigin } from "@/lib/public-url";

/**
 * Runtime proxy for OAuth requests to the Control Plane.
 * Same pattern as the main web app — reads CONTROL_PLANE_INTERNAL_URL at
 * request time, not build time.
 */

async function proxyToControlPlane(request: NextRequest, path: string): Promise<NextResponse> {
  const url = new URL(`/api/v1/auth/oauth/${path}`, getControlPlaneUrl());

  request.nextUrl.searchParams.forEach((value, key) => {
    url.searchParams.set(key, value);
  });

  // Tell the Control Plane to redirect back to this admin app after OAuth
  if (!path.includes("callback")) {
    url.searchParams.set("redirect_to", publicOrigin(request));
  }

  const headers: Record<string, string> = {};
  const cookie = request.headers.get("cookie");
  if (cookie) {
    headers.cookie = cookie;
  }

  const response = await fetch(url.toString(), {
    method: request.method,
    headers,
    redirect: "manual",
  });

  // Forward redirects (to Google/GitHub, then back)
  if (response.status >= 300 && response.status < 400) {
    const location = response.headers.get("location");
    if (location) {
      const res = NextResponse.redirect(location, response.status);
      const setCookies = response.headers.getSetCookie();
      for (const c of setCookies) {
        res.headers.append("set-cookie", c);
      }
      return res;
    }
  }

  const body = await response.text();
  return new NextResponse(body, {
    status: response.status,
    headers: {
      "content-type": response.headers.get("content-type") || "text/plain",
    },
  });
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
