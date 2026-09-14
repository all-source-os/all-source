import { fetchCatalog } from "@/lib/pricing-catalog";

// Client-facing proxy for the control-plane pricing catalog. The dashboard
// billing page (a client component) can't read CONTROL_PLANE_INTERNAL_URL, so it
// fetches here; the marketing /pricing page fetches `fetchCatalog()` directly in
// its server component. Do not cache an empty result after a control-plane failure.
export async function GET() {
  const catalog = await fetchCatalog();
  return Response.json(catalog ?? { currency: "USD", tiers: [] }, {
    headers: {
      "Cache-Control": catalog ? "public, max-age=300, stale-while-revalidate=3600" : "no-store",
    },
  });
}
