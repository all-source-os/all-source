import { NextResponse } from "next/server";
import { privateHeaders, queryConnection, remoteEnabled } from "@/lib/server/customer-agent-http";

export async function GET() {
  if (!remoteEnabled())
    return NextResponse.json({ error: "unavailable" }, { status: 404, headers: privateHeaders });
  try {
    const result = await queryConnection("oauth/resource");
    return NextResponse.json(result.data, { status: result.status, headers: privateHeaders });
  } catch {
    return NextResponse.json(
      { error: "temporarily_unavailable" },
      { status: 503, headers: privateHeaders }
    );
  }
}
