import { type NextRequest, NextResponse } from "next/server";
import {
  cookieOptions,
  historyConnect,
  historyEnabled,
  historyError,
  historyForm,
  historyHeaders,
  historyOrigin,
  historyQuery,
  historyRedirect,
  historyRequestCookie,
} from "@/lib/server/event-history-http";

type Context = { params: Promise<{ action: string }> };

export async function GET(request: NextRequest, { params }: Context) {
  if (!historyEnabled() || (await params).action !== "authorize") return historyError(404);
  if (!historyOrigin()) return historyError(503);
  const fields = request.nextUrl.searchParams;
  if (
    request.headers.has("authorization") ||
    request.nextUrl.search.length > 4096 ||
    new Set(fields.keys()).size !== fields.size
  )
    return historyError(400);
  try {
    const result = await historyQuery("oauth/prepare", Object.fromEntries(fields));
    if (
      !result.ok ||
      typeof result.data.request_token !== "string" ||
      !result.data.request_token ||
      result.data.request_token.length > 3800
    )
      return historyError(result.ok ? 502 : result.status);
    const response = historyRedirect(historyConnect);
    response.cookies.set(historyRequestCookie, result.data.request_token, {
      ...cookieOptions,
      path: historyConnect,
    });
    return response;
  } catch {
    return historyError(503);
  }
}

export async function POST(request: NextRequest, { params }: Context) {
  if (!historyEnabled() || (await params).action !== "token") return historyError(404);
  if (!historyOrigin() || request.nextUrl.search) return historyError(400);
  const basic = request.headers.get("authorization");
  if (basic && (basic.length > 4096 || !/^Basic [A-Za-z0-9+/]+=*$/i.test(basic)))
    return historyError(400);
  try {
    const form = await historyForm(request);
    if (!form) return historyError(400);
    const result = await historyQuery("oauth/token", form, basic ?? undefined);
    if (!result.ok) {
      const error = [
        "invalid_request",
        "invalid_client",
        "invalid_grant",
        "invalid_scope",
        "invalid_target",
        "unsupported_grant_type",
        "temporarily_unavailable",
      ].includes(result.data.error)
        ? result.data.error
        : "invalid_request";
      return NextResponse.json({ error }, { status: result.status, headers: historyHeaders });
    }
    const { access_token, token_type, expires_in, scope } = result.data;
    if (
      typeof access_token !== "string" ||
      token_type?.toLowerCase() !== "bearer" ||
      !Number.isSafeInteger(expires_in) ||
      expires_in < 1 ||
      scope !== "allsource.history.read"
    )
      return historyError(502);
    return NextResponse.json(
      { access_token, token_type, expires_in, scope },
      { headers: historyHeaders }
    );
  } catch {
    return historyError(503);
  }
}
