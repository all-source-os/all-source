export function GET() {
  return Response.json(
    { status: "ok", service: "allsource-admin" },
    { headers: { "cache-control": "no-store" } }
  );
}
