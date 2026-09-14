import { type NextRequest, NextResponse } from "next/server";
import { publicUrl } from "@/lib/public-url";

// Session tokens are minted by the Control Plane and validated by the Query
// Service's /api/v1/auth/me. NEXT_PUBLIC_API_URL points at the branded gateway
// (api.all-source.xyz), which routes /api/v1/events etc. to the QS but NOT
// /api/auth/* or /api/v1/auth/* — so token validation 404'd there and every
// login (Google/GitHub/email/demo) showed "Session expired". Resolve the QS
// directly; override with QUERY_SERVICE_URL.
function getQueryServiceUrl(): string {
  return (
    process.env.QUERY_SERVICE_URL ||
    (process.env.NODE_ENV === "production"
      ? "https://allsource-query.fly.dev"
      : "http://localhost:3902")
  );
}

// Only allow same-origin paths to defend against open-redirect via ?next=.
// Reject absolute URLs (//evil.com, https://...) and API/auth routes.
function safeNextPath(raw: string | null): string | null {
  if (!raw) return null;
  if (!raw.startsWith("/") || raw.startsWith("//")) return null;
  if (raw.startsWith("/api/") || raw.startsWith("/api?")) return null;
  return raw;
}

export async function GET(request: NextRequest) {
  const searchParams = request.nextUrl.searchParams;
  const token = searchParams.get("token");
  const error = searchParams.get("error");
  const isNewUser = searchParams.get("new_user") === "true";
  const nextPath = safeNextPath(searchParams.get("next"));

  // Handle OAuth errors
  if (error) {
    const loginUrl = publicUrl(request, "/login");
    loginUrl.searchParams.set("error", error);
    return NextResponse.redirect(loginUrl);
  }

  // Token is required
  if (!token) {
    const loginUrl = publicUrl(request, "/login");
    loginUrl.searchParams.set("error", "missing_token");
    return NextResponse.redirect(loginUrl);
  }

  // Verify token by fetching user info
  try {
    const meResponse = await fetch(`${getQueryServiceUrl()}/api/v1/auth/me`, {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    if (!meResponse.ok) {
      const loginUrl = publicUrl(request, "/login");
      loginUrl.searchParams.set("error", "invalid_token");
      return NextResponse.redirect(loginUrl);
    }

    // Token is valid, set cookie and redirect.
    // If the caller specified a safe ?next= target (e.g. /connect), honor it.
    // Otherwise fall back to onboarding (new user) or dashboard.
    const redirectUrl = nextPath ?? (isNewUser ? "/onboarding" : "/dashboard");
    const response = NextResponse.redirect(publicUrl(request, redirectUrl));

    // Set httpOnly cookie with the token
    response.cookies.set("auth_token", token, {
      httpOnly: true,
      secure: process.env.NODE_ENV === "production",
      sameSite: "lax",
      maxAge: 60 * 60 * 24 * 7, // 7 days
      path: "/",
    });

    return response;
  } catch {
    const loginUrl = publicUrl(request, "/login");
    loginUrl.searchParams.set("error", "auth_failed");
    return NextResponse.redirect(loginUrl);
  }
}
