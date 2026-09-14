export function GET() {
  return Response.json(
    { status: "ok", service: "allsource-web" },
    { headers: { "cache-control": "no-store" } }
  );
}
