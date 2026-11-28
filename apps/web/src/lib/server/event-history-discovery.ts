import { NextResponse } from "next/server";
import { boundedText, privateHeaders } from "@/lib/server/customer-agent-http";

export async function historyDiscovery(document: "resource" | "metadata") {
  const options = { headers: privateHeaders };
  if (process.env.ALLSOURCE_EVENT_HISTORY_ENABLED !== "true") {
    return NextResponse.json({ error: "unavailable" }, { ...options, status: 404 });
  }
  try {
    const base = process.env.QUERY_SERVICE_URL;
    if (!base) throw new Error("Query Service origin required");
    const deadline = AbortSignal.timeout(9000);
    const response = await fetch(new URL(`/api/event-history/oauth/${document}`, base), {
      cache: "no-store",
      redirect: "error",
      signal: deadline,
    });
    if (!response.ok) throw new Error("Discovery unavailable");
    const data = JSON.parse(await boundedText(response.body, 16384, deadline));
    return NextResponse.json(data, options);
  } catch {
    return NextResponse.json({ error: "temporarily_unavailable" }, { ...options, status: 503 });
  }
}
