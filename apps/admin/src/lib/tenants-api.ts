/**
 * Tenants API client for the admin dashboard.
 *
 * Fetches data from the Control Plane admin endpoint:
 *   GET /api/v1/admin/tenants?search=&plan=&status=&page=&per_page=
 */

function getApiUrl(): string {
  if (typeof window !== "undefined") {
    // Client-side: hit the same-origin BFF proxy (src/app/api/v1/[...path]/route.ts),
    // which attaches the admin_token Bearer and forwards to the Control Plane.
    return "";
  }
  return process.env.NEXT_PUBLIC_API_URL || "http://localhost:3902";
}

/**
 * Coerce a possibly-wrapped API response into an array (the shared §6 pattern —
 * see security-api.ts:26 / billing-api.ts:84). The Control Plane wraps list
 * responses inconsistently ({members:[…]}, {items}, {data}, or a bare array, and
 * sometimes a null); a non-array reaching a page's `.map` crashes the whole route
 * ("x.map is not a function"). Always resolve to an array.
 */
export function asList<T>(data: unknown, ...keys: string[]): T[] {
  if (Array.isArray(data)) return data as T[];
  if (data && typeof data === "object") {
    for (const k of [...keys, "items", "data"]) {
      const v = (data as Record<string, unknown>)[k];
      if (Array.isArray(v)) return v as T[];
    }
  }
  return [];
}

// Canonical subscription tiers (Control Plane authority — subscription.go).
// The CP filters the tenant list by exact case-insensitive match against these
// stored tier strings, so the admin filter MUST use the same names. The retired
// 010 aliases (starter/pro/growth/team) match nothing and are gone.
export type TenantPlan = "free" | "indie" | "studio" | "scale" | "enterprise";
export type TenantStatus = "active" | "suspended" | "archived";

// The CP list DTO emits `plan` as a bare tier string ("studio"); the DETAIL DTO
// emits it as an object {name, tier} (and `subscription.plan` may be either, or
// null). Rendering the object directly as a React child throws Minified React
// error #31 ("objects are not valid as a React child, found object with keys
// {name, tier}") — which crashed the whole tenant 360 page. Every plan render
// site MUST go through planLabel(); never render a plan value raw (resilience §6).
export type PlanLike = TenantPlan | { name?: string; tier?: string } | string | null | undefined;

export function planLabel(plan: PlanLike): string {
  if (plan == null) return "—";
  if (typeof plan === "string") return plan;
  // tier is the canonical id (matches TenantPlan + the `capitalize` styling); name is the fallback.
  return plan.tier ?? plan.name ?? "—";
}

export interface Tenant {
  id: string;
  name: string;
  plan: PlanLike;
  status: TenantStatus;
  // Per-tenant counts emitted by the CP list/detail DTOs under the SINGULAR
  // canonical field names `event_count` / `member_count` (prompt 033). The
  // earlier plural `events_count`/`members_count` never deserialized and always
  // rendered 0. Optional + guarded with `?? 0` at every render site (§6).
  event_count?: number;
  member_count?: number;
  created_at: string;
}

export interface TenantsResponse {
  tenants: Tenant[];
  total: number;
  page: number;
  per_page: number;
  _links: {
    self: string;
    next?: string;
    prev?: string;
  };
}

export interface FetchTenantsParams {
  search?: string;
  plan?: TenantPlan | "";
  status?: TenantStatus | "";
  page?: number;
  per_page?: number;
}

// ── Detail types ──────────────────────────────────────────────────────

export interface TenantMember {
  id: string;
  email: string;
  role: string;
  joined_at: string;
}

export interface TenantSubscription {
  plan: PlanLike;
  started_at: string;
  current_period_end: string;
  // Optional billing-state fields the 360 surfaces when the CP detail carries
  // them (admin_tenant_dto.go SubscriptionInfo). All optional + guarded.
  status?: string;
  provider?: string;
  renews_at?: string;
  dunning_state?: string;
  grandfathered?: boolean;
  grandfather_until?: string;
}

export interface TenantQuotas {
  event_limit: number;
  query_limit: number;
  storage_limit_mb: number;
}

/**
 * An API key surfaced on the tenant 360 (Pillar A — API keys section). The CP
 * detail may include the tenant's keys; the canonical role string MUST be
 * `serviceaccount` (no underscore) — a drifted `service_account` silently 403s
 * every key (MEMORY: API-key role string contract). All fields optional/guarded.
 */
export interface ApiKeyInfo {
  id: string;
  name?: string;
  role?: string;
  created_at?: string;
  last_used_at?: string;
}

export interface TenantDetail extends Tenant {
  description?: string;
  home_region?: string;
  quotas: TenantQuotas;
  members: TenantMember[];
  api_keys?: ApiKeyInfo[];
  subscription: TenantSubscription;
  audit_log: AuditEntry[];
}

export interface AuditEntry {
  id: string;
  action: string;
  actor: string;
  timestamp: string;
  details?: string;
}

// ── Usage types ───────────────────────────────────────────────────────

export interface DailyUsagePoint {
  date: string;
  events: number;
}

export interface TenantUsage {
  events_ingested: number;
  queries_run: number;
  storage_used_mb: number;
  event_limit: number;
  query_limit: number;
  storage_limit_mb: number;
  daily: DailyUsagePoint[];
}

// ── Detail fetchers ───────────────────────────────────────────────────

export async function fetchTenantDetail(id: string): Promise<TenantDetail> {
  const url = `${getApiUrl()}/api/v1/admin/tenants/${id}`;
  const res = await fetch(url, { credentials: "include" });
  if (!res.ok) {
    throw new Error(`Failed to fetch tenant detail: ${res.status}`);
  }
  const data = (await res.json()) ?? {};
  // Guard every list field so a wrapped/null payload can't crash the 360's .map (§6).
  return {
    ...data,
    members: asList<TenantMember>(data, "members"),
    api_keys: asList<ApiKeyInfo>(data, "api_keys", "keys"),
    audit_log: asList<AuditEntry>(data, "audit_log", "audit", "events"),
  };
}

// ── Per-tenant invoices (Pillar A — Subscription/billing section) ─────────
// Same-origin BFF read of the existing admin billing endpoint, filtered to this
// tenant. Mirrors billing-api.ts fetchInvoices but scoped + array-guarded so the
// 360 can show the tenant's recent invoices without importing the billing page.

export interface TenantInvoice {
  id: string;
  amount: number;
  currency: string;
  status: string;
  created_at: string;
}

export async function fetchTenantInvoices(id: string): Promise<TenantInvoice[]> {
  const url = `${getApiUrl()}/api/v1/admin/billing/invoices?tenant_id=${encodeURIComponent(id)}`;
  const res = await fetch(url, { credentials: "include" });
  if (!res.ok) {
    throw new Error(`Failed to fetch tenant invoices: ${res.status}`);
  }
  const data = (await res.json()) ?? {};
  return asList<TenantInvoice>(data, "invoices");
}

export async function fetchTenantUsage(id: string): Promise<TenantUsage> {
  const url = `${getApiUrl()}/api/v1/admin/tenants/${id}/usage`;
  const res = await fetch(url, { credentials: "include" });
  if (!res.ok) {
    throw new Error(`Failed to fetch tenant usage: ${res.status}`);
  }
  // The CP /usage payload uses Core-side names (event_count, query_count,
  // storage_bytes, daily_usage) and bytes; the UI type expects events_ingested /
  // queries_run / storage_used_mb / event_limit / daily. Map them here — passing
  // the raw payload through (the old behaviour) left every field undefined, which
  // is why the Usage-trends cards rendered 0 despite real metered counts (§6).
  const raw = (await res.json()) ?? {};
  const BYTES_PER_MB = 1024 * 1024;
  const dailySrc = Array.isArray(raw.daily_usage) ? raw.daily_usage : [];
  return {
    events_ingested: raw.event_count ?? 0,
    queries_run: raw.query_count ?? 0,
    storage_used_mb: Math.round((raw.storage_bytes ?? 0) / BYTES_PER_MB),
    event_limit: raw.event_limit ?? 0,
    query_limit: raw.query_limit ?? 0,
    storage_limit_mb: raw.storage_limit_mb ?? 0,
    daily: dailySrc.map((d: { date?: string; events_ingested?: number }) => ({
      date: d?.date ?? "",
      events: d?.events_ingested ?? 0,
    })),
  };
}

export async function updateTenantQuotas(id: string, quotas: TenantQuotas): Promise<void> {
  const url = `${getApiUrl()}/api/v1/admin/tenants/${id}/quotas`;
  const res = await fetch(url, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(quotas),
  });
  if (!res.ok) {
    throw new Error(`Failed to update quotas: ${res.status}`);
  }
}

export async function suspendTenant(id: string): Promise<void> {
  const url = `${getApiUrl()}/api/v1/admin/tenants/${id}/suspend`;
  const res = await fetch(url, {
    method: "POST",
    credentials: "include",
  });
  if (!res.ok) {
    throw new Error(`Failed to suspend tenant: ${res.status}`);
  }
}

export async function unsuspendTenant(id: string): Promise<void> {
  const url = `${getApiUrl()}/api/v1/admin/tenants/${id}/unsuspend`;
  const res = await fetch(url, {
    method: "POST",
    credentials: "include",
  });
  if (!res.ok) {
    throw new Error(`Failed to unsuspend tenant: ${res.status}`);
  }
}

// ── Registered client keys (operator-only ES256 registration) ─────────
// Routes: apps/control-plane/client_keys_admin.go. Registration is an operator
// action by design; tenants cannot register their own keys.

/** The managed key scopes the Control Plane accepts (managed_keys.go validKeyScopes). */
export const CLIENT_KEY_SCOPES = [
  "events:read",
  "events:write",
  "queries:execute",
  "projections:read",
  "projections:write",
  "schemas:read",
  "schemas:write",
] as const;

export type ClientKeyScope = (typeof CLIENT_KEY_SCOPES)[number];

export interface PublicJwk {
  kty: "EC";
  crv: "P-256";
  x: string;
  y: string;
}

export interface ClientKey {
  kid: string;
  revoked: boolean;
  invalid: boolean;
  active: boolean;
  client_id?: string;
  alg?: string;
  role?: string;
  scopes?: string[];
  created_at?: string;
  created_by?: string;
  revoked_at?: string;
  revoked_by?: string;
  jwk?: PublicJwk;
}

export type ClientKeyState = "active" | "revoked" | "invalid" | "inactive";

export function clientKeyState(key: ClientKey): ClientKeyState {
  if (key.revoked) return "revoked";
  if (key.invalid) return "invalid";
  return key.active ? "active" : "inactive";
}

export interface RegisterClientKeyRequest {
  client_id: string;
  jwk: PublicJwk;
  scopes: string[];
}

export interface RegisterClientKeyResult {
  kid: string;
  tenant_id: string;
  client_id: string;
  already_registered?: boolean;
  alg?: string;
  role?: string;
  scopes?: string[];
}

export interface RevokeClientKeyResult {
  kid: string;
  tenant_id: string;
  revoked_at?: string;
  already_revoked?: boolean;
}

const BASE64URL_32_BYTES = /^[A-Za-z0-9_-]{43}$/;

/**
 * Parse an operator-pasted JWK into the public members the register route
 * takes. Refuses a private JWK (one with `d`) before it leaves the browser.
 * The Control Plane still checks the point is on the curve.
 */
export function parsePublicJwk(text: string): PublicJwk {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error("The JWK is not valid JSON.");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("The JWK must be a JSON object.");
  }
  const jwk = value as Record<string, unknown>;
  if ("d" in jwk) {
    throw new Error("This is a private JWK. Paste the public key only (no d member).");
  }
  if (jwk.kty !== "EC" || jwk.crv !== "P-256") {
    throw new Error("The JWK must be a P-256 key: kty EC, crv P-256.");
  }
  if (
    typeof jwk.x !== "string" ||
    typeof jwk.y !== "string" ||
    !BASE64URL_32_BYTES.test(jwk.x) ||
    !BASE64URL_32_BYTES.test(jwk.y)
  ) {
    throw new Error("The JWK x and y must each be 32 bytes, base64url without padding.");
  }
  return { kty: "EC", crv: "P-256", x: jwk.x, y: jwk.y };
}

/** Prefer the Control Plane's `{error, message}` body over the bare status. */
async function clientKeyError(res: Response, action: string): Promise<string> {
  try {
    const data = await res.json();
    if (data && typeof data === "object") {
      if (typeof data.message === "string" && data.message) {
        return `Failed to ${action}: ${data.message}`;
      }
      if (typeof data.error === "string" && data.error) {
        return `Failed to ${action}: ${data.error}`;
      }
    }
  } catch {
    // non-JSON body: fall through to the status-only message
  }
  return `Failed to ${action}: ${res.status}`;
}

function clientKeysUrl(tenantId: string, kid?: string): string {
  const base = `${getApiUrl()}/api/v1/admin/tenants/${encodeURIComponent(tenantId)}/client-keys`;
  return kid ? `${base}/${encodeURIComponent(kid)}` : base;
}

export async function fetchClientKeys(tenantId: string): Promise<ClientKey[]> {
  const res = await fetch(clientKeysUrl(tenantId), { credentials: "include" });
  if (!res.ok) {
    throw new Error(await clientKeyError(res, "load client keys"));
  }
  const data = (await res.json()) ?? {};
  return asList<ClientKey>(data, "client_keys");
}

export async function registerClientKey(
  tenantId: string,
  request: RegisterClientKeyRequest
): Promise<RegisterClientKeyResult> {
  const res = await fetch(clientKeysUrl(tenantId), {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      client_id: request.client_id,
      jwk: request.jwk,
      scopes: request.scopes,
    }),
  });
  if (!res.ok) {
    throw new Error(await clientKeyError(res, "register client key"));
  }
  return res.json();
}

export async function revokeClientKey(
  tenantId: string,
  kid: string
): Promise<RevokeClientKeyResult> {
  const res = await fetch(clientKeysUrl(tenantId, kid), {
    method: "DELETE",
    credentials: "include",
  });
  if (!res.ok) {
    throw new Error(await clientKeyError(res, "revoke client key"));
  }
  return res.json();
}

// ── List fetcher ──────────────────────────────────────────────────────

export async function fetchTenants(params: FetchTenantsParams = {}): Promise<TenantsResponse> {
  const searchParams = new URLSearchParams();

  if (params.search) searchParams.set("search", params.search);
  if (params.plan) searchParams.set("plan", params.plan);
  if (params.status) searchParams.set("status", params.status);
  if (params.page) searchParams.set("page", String(params.page));
  if (params.per_page) searchParams.set("per_page", String(params.per_page));

  const qs = searchParams.toString();
  const url = `${getApiUrl()}/api/v1/admin/tenants${qs ? `?${qs}` : ""}`;

  const res = await fetch(url, { credentials: "include" });
  if (!res.ok) {
    throw new Error(`Failed to fetch tenants: ${res.status}`);
  }
  return res.json();
}
