import { type NextRequest, NextResponse } from "next/server";
import { decodeJwt, isAdminRole } from "@/lib/auth";
import { publicUrl } from "@/lib/public-url";

/**
 * Proxy that protects all authenticated routes.
 *
 * - Public routes (/login, /api/healthz, /api/auth/*, /api/v1/auth/*) are allowed through.
 * - The view-as API routes (/api/viewas/*) are allowed through here and enforce
 *   their OWN auth: start/stop/status read the admin_token cookie server-side and
 *   401 without it; the read-only data proxy reads the SEPARATE viewas_token and
 *   401s without it. They must not be redirected to the HTML /login page (they are
 *   JSON fetches), and the read-only data path is authorized by viewas_token, not
 *   admin_token — so the admin-role gate below would wrongly bounce it. The
 *   view-as PAGE (/view-as/*) is NOT exempted: it lives in the (authenticated)
 *   group and still requires a logged-in admin (admin_token) to even open the
 *   frame — only the data plane uses the scoped token.
 * - All other routes require a valid `admin_token` cookie with `role: "admin"`.
 * - Non-admin users receive a 403 response.
 * - Unauthenticated users are redirected to /login.
 *
 * This middleware runs in the Edge runtime: it uses ONLY `decodeJwt` (lib/auth.ts,
 * an atob+TextDecoder decoder) and Web APIs — never the Node base64 buffer API
 * (unavailable in Edge; it crashed the middleware for every authenticated request
 * when last used here, §6 rule 5).
 */
export function proxy(request: NextRequest) {
  const { pathname } = request.nextUrl;

  // Public routes — no auth required
  if (
    pathname === "/login" ||
    pathname === "/api/healthz" ||
    pathname.startsWith("/api/auth/") ||
    pathname.startsWith("/api/v1/auth/") ||
    pathname.startsWith("/api/viewas/") ||
    pathname.startsWith("/_next/") ||
    pathname.startsWith("/favicon")
  ) {
    return NextResponse.next();
  }

  const token = request.cookies.get("admin_token")?.value;

  // No token — redirect to login
  if (!token) {
    const loginUrl = publicUrl(request, "/login");
    loginUrl.searchParams.set("redirect", pathname);
    return NextResponse.redirect(loginUrl);
  }

  // Decode JWT and check admin role
  const payload = decodeJwt(token);

  if (!payload) {
    // Token is expired or malformed — clear cookie and redirect to login
    const loginUrl = publicUrl(request, "/login");
    loginUrl.searchParams.set("error", "invalid_token");
    const response = NextResponse.redirect(loginUrl);
    response.cookies.delete("admin_token");
    return response;
  }

  if (!isAdminRole(payload)) {
    // Valid token but not an admin — 403
    return new NextResponse(
      JSON.stringify({
        error: {
          code: "forbidden",
          message: "Admin access required. Your account does not have the admin role.",
        },
      }),
      {
        status: 403,
        headers: { "content-type": "application/json" },
      }
    );
  }

  // Admin token is valid — allow through
  return NextResponse.next();
}

export const config = {
  matcher: [
    /*
     * Match all request paths except static files and images.
     */
    "/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp)$).*)",
  ],
};
