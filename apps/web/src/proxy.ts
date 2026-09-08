import type { NextRequest } from "next/server";
import { NextResponse } from "next/server";
import { publicUrl } from "@/lib/public-url";

// Routes that require authentication
const protectedRoutes = ["/dashboard", "/onboarding"];

// Routes that are public
const _publicRoutes = [
  "/",
  "/login",
  "/signup",
  "/blog",
  "/api/auth",
  "/forgot-password",
  "/reset-password",
  "/verify-email",
];

export function proxy(request: NextRequest) {
  const { pathname } = request.nextUrl;
  const hostname = request.headers.get("host")?.split(":", 1)[0]?.toLowerCase();

  // Keep one canonical origin after moving DNS from Vercel to Fly.io.
  if (hostname === "all-source.xyz") {
    const canonicalUrl = request.nextUrl.clone();
    canonicalUrl.protocol = "https";
    canonicalUrl.hostname = "www.all-source.xyz";
    canonicalUrl.port = "";
    return NextResponse.redirect(canonicalUrl, 308);
  }

  // Check if the route is protected
  const isProtectedRoute = protectedRoutes.some((route) => pathname.startsWith(route));

  // Check if the route is public auth-related
  const isAuthRoute = pathname === "/login" || pathname === "/signup";

  // Get the auth token from cookies
  const token = request.cookies.get("auth_token")?.value;

  // If trying to access protected route without token, redirect to login
  if (isProtectedRoute && !token) {
    const loginUrl = publicUrl(request, "/login");
    loginUrl.searchParams.set("redirect", pathname);
    return NextResponse.redirect(loginUrl);
  }

  // If logged in and trying to access login/signup, redirect to dashboard
  if (isAuthRoute && token) {
    return NextResponse.redirect(publicUrl(request, "/dashboard"));
  }

  return NextResponse.next();
}

export const config = {
  matcher: [
    /*
     * Match all request paths except for the ones starting with:
     * - _next/static (static files)
     * - _next/image (image optimization files)
     * - favicon.ico (favicon file)
     * - public files (e.g. /images)
     */
    "/((?!_next/static|_next/image|favicon.ico|.*\\.(?:svg|png|jpg|jpeg|gif|webp)$).*)",
  ],
};
