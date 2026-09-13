// Pricing catalog — read from the control-plane `/api/v1/billing/catalog`
// endpoint, which sources live prices from LemonSqueezy (the source of truth for
// what customers are actually charged). Display prices must come from here, not
// from hardcoded numbers, so they can never drift from the real charge.
//
// `siteConfig.pricing` remains the source for tier METADATA (names, features,
// MCP verbs, x402 allowances). Paid prices never fall back to static config.

export type CatalogPrice = {
  cents: number;
  formatted: string; // monthly: per-month (e.g. "$18.99"); annual: total/yr (e.g. "$181.99")
  per_month?: string; // annual only: per-month equivalent (e.g. "$15.17")
};

export type CatalogTier = {
  tier: string;
  monthly?: CatalogPrice;
  annual?: CatalogPrice;
};

export type Catalog = {
  currency: string;
  tiers: CatalogTier[];
};

export type CatalogByTier = Record<string, CatalogTier>;

const CATALOG_FETCH_TIMEOUT_MS = 2_000;

function controlPlaneUrl(): string {
  return process.env.CONTROL_PLANE_INTERNAL_URL || "http://localhost:3901";
}

/**
 * Server-side fetch of the pricing catalog from the control plane.
 * On failure, returns null. Callers MUST NOT substitute config prices for a null paid-tier
 * price — render a dash via {@link resolveMonthly}/{@link resolveYearlyPerMonth}.
 * No second cache here; the control plane caches Lemon Squeezy for five minutes.
 */
export async function fetchCatalog(): Promise<Catalog | null> {
  try {
    const res = await fetch(`${controlPlaneUrl()}/api/v1/billing/catalog`, {
      cache: "no-store",
      signal: AbortSignal.timeout(CATALOG_FETCH_TIMEOUT_MS),
    });
    if (!res.ok) return null;
    const catalog = (await res.json()) as Catalog;
    return catalog?.tiers?.length ? catalog : null;
  } catch {
    return null;
  }
}

// PriceUnavailable is shown when a paid tier's live price can't be resolved
// from Lemon Squeezy. Never fall back to a config number.
export const PriceUnavailable = "—";

/**
 * isFixedConfigPrice reports whether a config price string is fixed metadata
 * (not priced by LemonSqueezy) — Custom enterprise, plus the legacy "$0"/"Free"
 * self-host string. Those are authoritative from config and can't drift, so
 * they're safe to display as-is. NOTE: Self-Host is no longer marketed as a free
 * plan (it's gone from /pricing), so the "$0"/"Free" branch now only ever fires
 * on the authenticated dashboard when rendering a tenant still on the legacy
 * self-host tier — never on a public pricing surface.
 */
export function isFixedConfigPrice(price: string | undefined): boolean {
  return price === "$0" || price === "Free" || price === "Custom";
}

/**
 * resolveMonthly returns the monthly price string to display for a tier. Fixed
 * tiers return their config price; paid tiers return the live/last-good catalog
 * price, or {@link PriceUnavailable} — NEVER the config price.
 */
export function resolveMonthly(cat: CatalogTier | undefined, configPrice: string): string {
  if (isFixedConfigPrice(configPrice)) return configPrice;
  return cat?.monthly?.formatted ?? PriceUnavailable;
}

/** resolveYearlyPerMonth is {@link resolveMonthly} for the per-month annual view. */
export function resolveYearlyPerMonth(cat: CatalogTier | undefined, configPrice: string): string {
  if (isFixedConfigPrice(configPrice)) return configPrice;
  return cat?.annual?.per_month ?? PriceUnavailable;
}

/** resolveAnnualTotal returns the live annual total, or undefined (no config fallback). */
export function resolveAnnualTotal(cat: CatalogTier | undefined): string | undefined {
  return cat?.annual?.formatted;
}

/** The amount actually charged for the selected billing period, never an annual /12 estimate. */
export function resolveBilledPrice(
  cat: CatalogTier | undefined,
  configPrice: string,
  period: "monthly" | "annual"
): string {
  if (isFixedConfigPrice(configPrice)) return configPrice;
  return period === "annual"
    ? (resolveAnnualTotal(cat) ?? PriceUnavailable)
    : resolveMonthly(cat, configPrice);
}

/** Carry the selected tier and period through sign-up to the billing page. */
export function pricingSignupHref(tier: string, period: "monthly" | "annual"): string {
  const destination = `/dashboard/billing?plan=${encodeURIComponent(tier)}&period=${period}`;
  return `/signup?next=${encodeURIComponent(destination)}`;
}

/** Index a catalog by tier id for O(1) lookup; tolerant of null. */
export function indexByTier(catalog: Catalog | null): CatalogByTier {
  const map: CatalogByTier = {};
  for (const t of catalog?.tiers ?? []) map[t.tier] = t;
  return map;
}
