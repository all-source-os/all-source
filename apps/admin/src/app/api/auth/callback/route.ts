import { type NextRequest, NextResponse } from "next/server";
import { validateAdminToken } from "@/lib/auth";
import { publicUrl } from "@/lib/public-url";

export async function GET(request: NextRequest) {
  const searchParams = request.nextUrl.searchParams;
  const token = searchParams.get("token");
  const error = searchParams.get("error");

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

  // Validate token and check admin role
  const result = await validateAdminToken(token);

  if (!result.valid) {
    const loginUrl = publicUrl(request, "/login");
    loginUrl.searchParams.set("error", result.error);
    return NextResponse.redirect(loginUrl);
  }

  // Token is valid and user is admin — set cookie and redirect
  const response = NextResponse.redirect(publicUrl(request, "/tenants"));

  response.cookies.set("admin_token", token, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    maxAge: 60 * 60 * 24 * 7, // 7 days
    path: "/",
  });

  return response;
}
