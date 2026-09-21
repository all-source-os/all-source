// Canonical tier ids — the ONE naming scheme used everywhere in the app.
// `self-host` is the free, run-it-yourself tier; indie/studio/scale are the
// paid hosted tiers; enterprise is sales-led.
export type CanonicalTier = "trial" | "self-host" | "indie" | "studio" | "scale" | "enterprise";

// retiredToCanonical is the SINGLE retired→canonical map on the web side — the
// mirror of the backend's MapRetiredTier, applied once at the ingest edge where
// a raw `subscription_tier` enters from the API. Everything downstream works in
// canonical ids only (no more matching both `tier` and a legacy `billingTier`).
//
// Legacy backend `subscription_tier` values: free / starter / growth / team /
// pro / developer. New writes are already canonical; this covers historical
// rows that predate the backend canonicalization.
const retiredToCanonical: Record<string, CanonicalTier> = {
  free: "self-host",
  starter: "indie",
  developer: "indie",
  pro: "indie",
  growth: "studio",
  team: "studio",
};

// canonicalTier maps a raw backend tier string to a canonical tier id. Canonical
// ids pass through unchanged; unknown values fall back to "self-host" (the free
// tier) so the UI degrades to the lowest plan rather than crashing.
export function canonicalTier(raw: string | null | undefined): CanonicalTier {
  if (!raw) return "self-host";
  const t = raw.trim().toLowerCase();
  if (t === "trial") return "trial";
  if (t === "self-host" || t === "indie" || t === "studio" || t === "scale" || t === "enterprise") {
    return t;
  }
  return retiredToCanonical[t] ?? "self-host";
}

// TIER_RANK orders the canonical tiers for upgrade/downgrade comparisons.
export const TIER_RANK: Record<CanonicalTier, number> = {
  trial: 0,
  "self-host": 0,
  indie: 1,
  studio: 2,
  scale: 3,
  enterprise: 4,
};

// tierRank returns the rank of a raw (possibly legacy) tier string.
export function tierRank(raw: string | null | undefined): number {
  return TIER_RANK[canonicalTier(raw)];
}
