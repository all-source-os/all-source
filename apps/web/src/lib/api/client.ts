// API Client for Query Service
// All dashboard operations go through this client.
// Uses relative URLs so browser requests go through the Next.js API proxy,
// avoiding CORS issues. Server-side code should use getServerApiUrl() for absolute URLs.

export interface ApiError {
  code: string;
  message: string;
  details?: Record<string, string[]>;
}

export interface ApiResponse<T> {
  data?: T;
  error?: ApiError;
}

export function normalizeApiError(value: unknown, fallback: string): ApiError {
  if (typeof value === "string") {
    return { code: "request_failed", message: value };
  }

  if (value && typeof value === "object") {
    const error = value as Record<string, unknown>;
    return {
      code: typeof error.code === "string" ? error.code : "request_failed",
      message: typeof error.message === "string" ? error.message : fallback,
      ...(error.details && typeof error.details === "object"
        ? { details: error.details as Record<string, string[]> }
        : {}),
    };
  }

  return { code: "unknown_error", message: fallback };
}

/** Per-tenant schema-enforcement mode (Gap 3). */
export type SchemaEnforcementMode = "permissive" | "warn" | "strict";

export class ApiClient {
  private baseUrl: string;
  private asOf: string | null = null;

  constructor(baseUrl: string = "") {
    this.baseUrl = baseUrl;
  }

  /**
   * Set the as_of timestamp for time travel queries.
   * All subsequent requests will include this timestamp.
   * Set to null to return to present.
   */
  setAsOf(timestamp: string | null): void {
    this.asOf = timestamp;
  }

  /**
   * Get the current as_of timestamp.
   */
  getAsOf(): string | null {
    return this.asOf;
  }

  private async request<T>(endpoint: string, options: RequestInit = {}): Promise<ApiResponse<T>> {
    // Append as_of parameter if set and not already present
    let url = `${this.baseUrl}${endpoint}`;
    if (this.asOf && !endpoint.includes("as_of=")) {
      const separator = endpoint.includes("?") ? "&" : "?";
      url = `${url}${separator}as_of=${encodeURIComponent(this.asOf)}`;
    }

    const headers: HeadersInit = {
      "Content-Type": "application/json",
      ...options.headers,
    };

    try {
      const response = await fetch(url, {
        ...options,
        headers,
        credentials: "include", // Include cookies for auth
      });

      const text = await response.text();
      let data: Record<string, unknown>;
      try {
        data = text ? JSON.parse(text) : {};
      } catch {
        data = {};
      }

      if (!response.ok) {
        return {
          error: normalizeApiError(data.error, response.statusText || `HTTP ${response.status}`),
        };
      }

      return { data: (data.data ?? data) as T };
    } catch (error) {
      return {
        error: {
          code: "network_error",
          message: error instanceof Error ? error.message : "Network error",
        },
      };
    }
  }

  // Auth endpoints
  async getMe(): Promise<ApiResponse<User>> {
    return this.request<User>("/api/auth/me");
  }

  async logout(): Promise<ApiResponse<void>> {
    return this.request<void>("/api/auth/logout", { method: "POST" });
  }

  // Tenant endpoints
  async getTenant(): Promise<ApiResponse<Tenant>> {
    return this.request<Tenant>("/api/tenant");
  }

  async updateTenant(data: UpdateTenantRequest): Promise<ApiResponse<Tenant>> {
    return this.request<Tenant>("/api/tenant", {
      method: "PUT",
      body: JSON.stringify(data),
    });
  }

  async getTenantUsage(): Promise<ApiResponse<TenantUsage>> {
    return this.request<TenantUsage>("/api/tenant/usage");
  }

  // Schema-enforcement toggle (Gap 3) — proxied to Core's per-tenant setting.
  async getSchemaEnforcement(): Promise<
    ApiResponse<{ schema_enforcement: SchemaEnforcementMode }>
  > {
    return this.request("/api/tenant/schema-enforcement");
  }

  async setSchemaEnforcement(
    mode: SchemaEnforcementMode
  ): Promise<ApiResponse<{ schema_enforcement: SchemaEnforcementMode }>> {
    return this.request("/api/tenant/schema-enforcement", {
      method: "PUT",
      body: JSON.stringify({ schema_enforcement: mode }),
    });
  }

  // Events endpoints
  async createEvent(event: CreateEventRequest): Promise<ApiResponse<Event>> {
    return this.request<Event>("/api/events", {
      method: "POST",
      body: JSON.stringify(event),
    });
  }

  async createEventBatch(events: CreateEventRequest[]): Promise<ApiResponse<Event[]>> {
    return this.request<Event[]>("/api/events/batch", {
      method: "POST",
      body: JSON.stringify({ events }),
    });
  }

  async listEvents(params?: ListEventsParams): Promise<ApiResponse<EventListResponse>> {
    const queryString = params
      ? `?${new URLSearchParams(
          Object.entries(params).reduce(
            (acc, [key, value]) => {
              if (value !== undefined) acc[key] = String(value);
              return acc;
            },
            {} as Record<string, string>
          )
        ).toString()}`
      : "";
    return this.request<EventListResponse>(`/api/events${queryString}`);
  }

  async getEventsByEntity(entityId: string): Promise<ApiResponse<EventListResponse>> {
    return this.request<EventListResponse>(`/api/events/entity/${encodeURIComponent(entityId)}`);
  }

  async getEventsByType(eventType: string): Promise<ApiResponse<EventListResponse>> {
    return this.request<EventListResponse>(`/api/events/type/${encodeURIComponent(eventType)}`);
  }

  // Streams (entity discovery) endpoint
  async listStreams(params?: {
    limit?: number;
    offset?: number;
  }): Promise<ApiResponse<StreamsListResponse>> {
    const queryString = params
      ? `?${new URLSearchParams(
          Object.entries(params).reduce(
            (acc, [key, value]) => {
              if (value !== undefined) acc[key] = String(value);
              return acc;
            },
            {} as Record<string, string>
          )
        ).toString()}`
      : "";
    return this.request<StreamsListResponse>(`/api/streams${queryString}`);
  }

  // Event types discovery endpoint
  async listEventTypes(params?: {
    limit?: number;
    offset?: number;
  }): Promise<ApiResponse<EventTypesListResponse>> {
    const queryString = params
      ? `?${new URLSearchParams(
          Object.entries(params).reduce(
            (acc, [key, value]) => {
              if (value !== undefined) acc[key] = String(value);
              return acc;
            },
            {} as Record<string, string>
          )
        ).toString()}`
      : "";
    return this.request<EventTypesListResponse>(`/api/event-types${queryString}`);
  }

  // Query endpoint
  async executeQuery(query: QueryRequest): Promise<ApiResponse<QueryResponse>> {
    return this.request<QueryResponse>("/api/query", {
      method: "POST",
      body: JSON.stringify(query),
    });
  }

  // API Keys endpoints
  async listApiKeys(): Promise<ApiResponse<ApiKey[]>> {
    // Backend wraps response in {keys: [...], count: N}
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const response = await this.request<any>("/api/api-keys");
    if (response.data?.keys && Array.isArray(response.data.keys)) {
      return { data: response.data.keys };
    }
    return { data: response.data ?? [] };
  }

  async createApiKey(data: CreateApiKeyRequest): Promise<ApiResponse<ApiKeyWithSecret>> {
    return this.request<ApiKeyWithSecret>("/api/api-keys", {
      method: "POST",
      body: JSON.stringify(data),
    });
  }

  /**
   * Claim an anonymous-trial tenant into the calling authenticated user.
   * Closes bead t-e8b8 — the web-side counterpart to the control-plane
   * POST /api/v1/agents/claim endpoint. Trial tenants are minted via
   * POST /api/v1/agents/anonymous-trial; their claim_token is delivered
   * to the human via the article install protocol, which then surfaces
   * it here when the human visits /connect?claim=<token>.
   */
  async claimTrialAgent(claimToken: string): Promise<ApiResponse<ClaimTrialResult>> {
    return this.request<ClaimTrialResult>("/api/v1/agents/claim", {
      method: "POST",
      body: JSON.stringify({ claim_token: claimToken }),
    });
  }

  async getApiKey(id: string): Promise<ApiResponse<ApiKey>> {
    return this.request<ApiKey>(`/api/api-keys/${id}`);
  }

  async updateApiKey(id: string, data: UpdateApiKeyRequest): Promise<ApiResponse<ApiKey>> {
    return this.request<ApiKey>(`/api/api-keys/${id}`, {
      method: "PUT",
      body: JSON.stringify(data),
    });
  }

  async rotateApiKey(id: string): Promise<ApiResponse<ApiKeyWithSecret>> {
    return this.request<ApiKeyWithSecret>(`/api/api-keys/${id}/rotate`, {
      method: "POST",
    });
  }

  async revokeApiKey(id: string): Promise<ApiResponse<void>> {
    return this.request<void>(`/api/api-keys/${id}`, {
      method: "DELETE",
    });
  }

  async getApiKeyScopes(): Promise<ApiResponse<string[]>> {
    return this.request<string[]>("/api/api-keys/scopes");
  }

  // Billing endpoints
  async getBillingStatus(tenantId?: string): Promise<ApiResponse<BillingStatus>> {
    const qs = tenantId ? `?tenant_id=${encodeURIComponent(tenantId)}` : "";
    return this.request<BillingStatus>(`/api/billing/status${qs}`);
  }

  async createCheckout(
    tier: string,
    billingPeriod: "monthly" | "annual" = "monthly",
    options?: { tenantId?: string; email?: string; redirectUrl?: string }
  ): Promise<ApiResponse<CheckoutResponse>> {
    return this.request<CheckoutResponse>("/api/billing/checkout", {
      method: "POST",
      body: JSON.stringify({
        tenant_id: options?.tenantId,
        tier,
        billing_period: billingPeriod,
        email: options?.email,
        redirect_url: options?.redirectUrl,
      }),
    });
  }

  // In-place plan change for an existing subscriber (no new checkout).
  async changePlan(
    tier: string,
    billingPeriod: "monthly" | "annual" = "monthly"
  ): Promise<ApiResponse<{ tier: string; subscription_id: string }>> {
    return this.request<{ tier: string; subscription_id: string }>("/api/billing/change-plan", {
      method: "POST",
      body: JSON.stringify({ tier, billing_period: billingPeriod }),
    });
  }

  async getBillingPortal(tenantId?: string): Promise<ApiResponse<BillingPortalResponse>> {
    const qs = tenantId ? `?tenant_id=${encodeURIComponent(tenantId)}` : "";
    return this.request<BillingPortalResponse>(`/api/billing/portal${qs}`);
  }

  async getOverage(): Promise<ApiResponse<OverageResponse>> {
    return this.request<OverageResponse>("/api/billing/overage");
  }

  async enableOverage(rates?: OverageRates): Promise<ApiResponse<void>> {
    return this.request<void>("/api/billing/overage/enable", {
      method: "POST",
      body: JSON.stringify(rates || {}),
    });
  }

  async disableOverage(): Promise<ApiResponse<void>> {
    return this.request<void>("/api/billing/overage/disable", {
      method: "POST",
    });
  }

  async getProjectedCharges(): Promise<ApiResponse<ProjectedChargesResponse>> {
    return this.request<ProjectedChargesResponse>("/api/billing/projected-charges");
  }

  // Projections endpoints (per-tenant, QS-owned; tenant-scoped + fail-closed)
  async listProjections(): Promise<ApiResponse<ProjectionListResponse>> {
    return this.request<ProjectionListResponse>("/api/projections");
  }

  async listProjectionTemplates(): Promise<ApiResponse<ProjectionTemplateListResponse>> {
    return this.request<ProjectionTemplateListResponse>("/api/projection-templates");
  }

  async enableProjection(template: string): Promise<ApiResponse<{ projection: Projection }>> {
    return this.request<{ projection: Projection }>("/api/projections", {
      method: "POST",
      body: JSON.stringify({ template }),
    });
  }

  async disableProjection(name: string): Promise<ApiResponse<{ deleted: string }>> {
    return this.request<{ deleted: string }>(`/api/projections/${encodeURIComponent(name)}`, {
      method: "DELETE",
    });
  }

  /**
   * Read a projection's folded read-model state for one entity bucket.
   *
   * Tenant-scoped + fail-closed at the QS boundary. `entityId` selects the ETS
   * bucket: tenant-wide templates (counter / breakdown / timeseries, and the
   * active-entities summary) live under the synthetic `_tenant` key, which the
   * server defaults to when `entity_id` is omitted; per-entity templates
   * (entity-activity) require the caller to pass the entity_id. Returns 404 when
   * the projection is not enabled for the tenant or there is no state for that
   * bucket yet (still building / no data folded).
   */
  async getProjectionState(
    name: string,
    entityId?: string
  ): Promise<ApiResponse<ProjectionStateResponse>> {
    const qs = entityId ? `?entity_id=${encodeURIComponent(entityId)}` : "";
    return this.request<ProjectionStateResponse>(
      `/api/projections/${encodeURIComponent(name)}/state${qs}`
    );
  }

  // Metrics endpoints
  async getMetrics(): Promise<ApiResponse<MetricsResponse>> {
    return this.request<MetricsResponse>("/api/metrics");
  }

  // Analytics endpoints
  async getAnalyticsStats(params?: { as_of?: string }): Promise<ApiResponse<AnalyticsStats>> {
    const queryString = params?.as_of ? `?as_of=${encodeURIComponent(params.as_of)}` : "";
    return this.request<AnalyticsStats>(`/api/analytics/stats${queryString}`);
  }

  // Events replay for time range
  async getEventsInRange(params: {
    start: string;
    end: string;
    entity_id?: string;
    event_type?: string;
    limit?: number;
  }): Promise<ApiResponse<EventListResponse>> {
    const queryParams = new URLSearchParams();
    queryParams.set("start", params.start);
    queryParams.set("end", params.end);
    if (params.entity_id) queryParams.set("entity_id", params.entity_id);
    if (params.event_type) queryParams.set("event_type", params.event_type);
    if (params.limit) queryParams.set("limit", String(params.limit));

    return this.request<EventListResponse>(`/api/events/range?${queryParams.toString()}`);
  }

  // Team management endpoints
  async listTeamMembers(): Promise<ApiResponse<TeamMembersResponse>> {
    return this.request<TeamMembersResponse>("/api/team/members");
  }

  async inviteTeamMember(data: InviteMemberRequest): Promise<ApiResponse<Invitation>> {
    return this.request<Invitation>("/api/team/invite", {
      method: "POST",
      body: JSON.stringify(data),
    });
  }

  async removeTeamMember(userId: string): Promise<ApiResponse<void>> {
    return this.request<void>(`/api/team/members/${userId}`, {
      method: "DELETE",
    });
  }

  async updateTeamMemberRole(userId: string, role: string): Promise<ApiResponse<TeamMember>> {
    return this.request<TeamMember>(`/api/team/members/${userId}/role`, {
      method: "PUT",
      body: JSON.stringify({ role }),
    });
  }

  // Agent key management
  async listAgentKeys(): Promise<ApiResponse<AgentKeysResponse>> {
    return this.request<AgentKeysResponse>("/api/team/agent-keys");
  }

  async createAgentKey(name: string): Promise<ApiResponse<AgentKeyCreated>> {
    return this.request<AgentKeyCreated>("/api/team/agent-keys", {
      method: "POST",
      body: JSON.stringify({ name }),
    });
  }

  async revokeAgentKey(name: string): Promise<ApiResponse<void>> {
    return this.request<void>(`/api/team/agent-keys/${encodeURIComponent(name)}`, {
      method: "DELETE",
    });
  }

  // Audit log endpoints
  async listAuditLogs(params?: AuditLogParams): Promise<ApiResponse<AuditLogResponse>> {
    const queryString = params
      ? `?${new URLSearchParams(
          Object.entries(params).reduce(
            (acc, [key, value]) => {
              if (value !== undefined) acc[key] = String(value);
              return acc;
            },
            {} as Record<string, string>
          )
        ).toString()}`
      : "";
    return this.request<AuditLogResponse>(`/api/tenant/audit-logs${queryString}`);
  }

  // Usage analytics endpoints
  async getUsageAnalytics(params?: {
    range?: string;
  }): Promise<ApiResponse<UsageAnalyticsResponse>> {
    const queryString = params?.range ? `?range=${encodeURIComponent(params.range)}` : "";
    return this.request<UsageAnalyticsResponse>(`/api/tenants/me/analytics${queryString}`);
  }

  // Replay endpoints
  async analyzeReplay(params: { projection_name: string }): Promise<ApiResponse<ReplayAnalysis>> {
    return this.request<ReplayAnalysis>("/api/replay/preview", {
      method: "POST",
      body: JSON.stringify(params),
    });
  }

  async startReplay(params: StartReplayRequest): Promise<ApiResponse<ReplayProgress>> {
    return this.request<ReplayProgress>("/api/replay", {
      method: "POST",
      body: JSON.stringify(params),
    });
  }

  async listReplays(): Promise<ApiResponse<ReplayListResponse>> {
    return this.request<ReplayListResponse>("/api/replay");
  }

  async getReplay(replayId: string): Promise<ApiResponse<ReplayProgress>> {
    return this.request<ReplayProgress>(`/api/replay/${replayId}`);
  }

  async cancelReplay(replayId: string): Promise<ApiResponse<ReplayProgress>> {
    return this.request<ReplayProgress>(`/api/replay/${replayId}/cancel`, {
      method: "POST",
    });
  }

  async deleteReplay(replayId: string): Promise<ApiResponse<{ deleted: boolean }>> {
    return this.request<{ deleted: boolean }>(`/api/replay/${replayId}`, {
      method: "DELETE",
    });
  }

  // Prime knowledge-graph endpoint (allsource-prime, proxied through Core).
  // Returns the tenant's full node+edge graph. Supports node_type + limit
  // filters that are applied server-side (before the limit is reached).
  async getPrimeGraph(params?: PrimeGraphParams): Promise<ApiResponse<PrimeGraph>> {
    const queryString = params
      ? `?${new URLSearchParams(
          Object.entries(params).reduce(
            (acc, [key, value]) => {
              if (value !== undefined) acc[key] = String(value);
              return acc;
            },
            {} as Record<string, string>
          )
        ).toString()}`
      : "";
    return this.request<PrimeGraph>(`/api/v1/prime/graph${queryString}`);
  }

  // Entity timeline (formatted for visualization)
  async getEntityTimeline(
    entityId: string,
    params?: { as_of?: string }
  ): Promise<ApiResponse<EntityTimeline>> {
    const queryString = params?.as_of ? `?as_of=${encodeURIComponent(params.as_of)}` : "";
    return this.request<EntityTimeline>(
      `/api/entities/${encodeURIComponent(entityId)}/timeline${queryString}`
    );
  }
}

// Types
export interface User {
  id: string;
  email: string;
  name: string;
  avatar_url: string | null;
  provider: "google" | "github" | "email";
  email_verified: boolean;
  tenant_id: string;
}

export interface Tenant {
  id: string;
  name: string;
  slug: string;
  subscription_status: "active" | "trialing" | "past_due" | "cancelled" | "expired";
  // Raw backend tier. Canonical ids are self-host|indie|studio|scale|enterprise,
  // but historical rows may still carry legacy values (free|starter|growth|team|
  // pro). Always normalize with canonicalTier() from "@/lib/tier" before use.
  subscription_tier: string;
  billing_period: "monthly" | "annual" | null;
  trial_ends_at: string | null;
  subscription_ends_at: string | null;
  events_quota: number;
  queries_quota: number;
  events_used: number;
  queries_used: number;
  is_demo?: boolean;
  settings: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface UpdateTenantRequest {
  name?: string;
  settings?: Record<string, unknown>;
}

export interface TenantUsage {
  tenant_id: string;
  subscription_tier: string;
  subscription_status: string;
  events: {
    used: number;
    quota: number;
    percentage: number;
  };
  queries: {
    used: number;
    quota: number;
    percentage: number;
  };
  /** Hosted Hound extraction LLM tokens this period. quota -1 = unlimited, 0 = none included. */
  extraction_tokens?: {
    used: number;
    quota: number;
    remaining: number;
  };
  billing_period: {
    reset_at: string;
  };
}

export interface Event {
  id: string;
  entity_id: string;
  event_type: string;
  payload: Record<string, unknown>;
  timestamp: string;
  version: number;
}

export interface CreateEventRequest {
  entity_id: string;
  event_type: string;
  payload: Record<string, unknown>;
}

export interface ListEventsParams {
  entity_id?: string;
  event_type?: string;
  /** Server-side prefix filter, e.g. "prime." to fetch only Prime memory events. */
  event_type_prefix?: string;
  /**
   * Server-side EXCLUDE filter — comma-separated prefixes dropped before the
   * limit, e.g. "audit.,service.,_system." to hide operational namespaces from
   * a recent-activity feed so they can't bury domain events.
   */
  exclude_event_type_prefix?: string;
  limit?: number;
  offset?: number;
  as_of?: string;
}

export interface EventListResponse {
  data: Event[];
  count: number;
}

export interface StreamInfo {
  stream_id: string;
  event_count: number;
  last_event_at: string | null;
}

export interface StreamsListResponse {
  data: StreamInfo[];
  count: number;
  total: number;
}

export interface EventTypeInfo {
  event_type: string;
  event_count: number;
  last_event_at: string | null;
}

export interface EventTypesListResponse {
  data: EventTypeInfo[];
  count: number;
  total: number;
}

export interface QueryRequest {
  entity_id?: string;
  event_type?: string;
  from?: string;
  where?: QueryCondition;
  limit?: number;
  offset?: number;
}

export interface QueryCondition {
  op?: string;
  field?: string;
  value?: unknown;
  and?: QueryCondition[];
  or?: QueryCondition[];
}

export interface QueryResponse {
  data: Event[];
  count: number;
  query: {
    from: string;
    has_filter: boolean;
    limit: number;
    offset: number;
    type: string;
  };
}

export interface ApiKey {
  id: string;
  name: string;
  description: string | null;
  key_prefix: string;
  scopes: string[];
  last_used_at: string | null;
  expires_at: string | null;
  created_at: string;
}

export interface ApiKeyWithSecret extends ApiKey {
  key: string;
}

/**
 * Response shape from POST /api/v1/agents/claim. Mirrors the control-plane
 * dto.ClaimTrialAgentResponse — see
 * apps/control-plane/internal/application/dto/agent_dto.go.
 * events_migrated is always false in v1; trial events stay under the
 * original tenant_id and a future gateway-resolved mapping reads
 * claimed_by_tenant when serving them.
 */
export interface ClaimTrialResult {
  trial_tenant_id: string;
  claimed_at: string;
  claimed_by_tenant: string;
  events_migrated: boolean;
  events_migration_note?: string;
}

export interface CreateApiKeyRequest {
  name: string;
  description?: string;
  scopes: string[];
  expires_at?: string;
}

export interface UpdateApiKeyRequest {
  name?: string;
  description?: string;
  scopes?: string[];
}

export interface CheckoutResponse {
  checkout_id: string;
  checkout_url: string;
  tenant_id: string;
  tier: string;
  provider: string;
}

export interface BillingStatus {
  tenant_id: string;
  tier: "free" | "pro" | "growth" | "enterprise";
  status: "active" | "trialing" | "past_due" | "cancelled" | "expired";
  billing_period: "monthly" | "annual" | null;
  payment_provider: string | null;
  subscription_id: string | null;
  events_quota: number;
  queries_quota: number;
  events_used: number;
  queries_used: number;
  last_updated: string | null;
}

export interface BillingPortalResponse {
  portal_url?: string;
  tenant_id?: string;
}

export interface OverageResponse {
  overage_enabled: boolean;
  events: {
    used: number;
    quota: number;
    overage: number;
    rate_cents: number;
  };
  queries: {
    used: number;
    quota: number;
    overage: number;
    rate_cents: number;
  };
  estimated_charges_cents: number;
}

export interface OverageRates {
  events_rate?: number;
  queries_rate?: number;
}

export interface ProjectedChargesResponse {
  subscription_tier: string;
  overage_enabled: boolean;
  usage: TenantUsage;
  overage_charges: {
    events_overage: number;
    queries_overage: number;
    events_charges_cents: number;
    queries_charges_cents: number;
    total_charges_cents: number;
  };
  billing_period: {
    reset_at: string;
  };
}

// Per-tenant projections are a Query-Service-owned read-model: a tenant enables
// curated templates and QS folds the tenant's event stream into state. `status`
// is "building" while the background backfill runs, then "ready".
//
// `kind` is the render hint the dashboard switches on to format folded state
// WITHOUT knowing the template name:
//   - "counter"      single headline number + optional by-key breakdown
//   - "breakdown"    label -> count map (sorted table / bars)
//   - "timeseries"   UTC-day -> count map (compact chart)
//   - "entity_table" per-entity rows (small table, optional entity_id filter)
export type ProjectionKind = "counter" | "breakdown" | "timeseries" | "entity_table";

export interface Projection {
  name: string;
  title: string;
  description: string | null;
  kind: ProjectionKind | null;
  status: "building" | "ready";
}

export interface ProjectionListResponse {
  projections: Projection[];
  total: number;
}

export interface ProjectionTemplate {
  name: string;
  title: string;
  description: string;
  kind: ProjectionKind | null;
}

export interface ProjectionTemplateListResponse {
  templates: ProjectionTemplate[];
  total: number;
}

// Folded read-model returned by GET /api/projections/{name}/state. `state` is the
// raw folded value whose shape depends on `kind` (see ProjectionStateView).
export interface ProjectionStateResponse {
  projection: string;
  entity_id: string;
  kind: ProjectionKind | null;
  status: "building" | "ready";
  state: Record<string, unknown>;
}

export interface MetricsResponse {
  service: string;
  timestamp: string;
  elixir: {
    processes: number;
    memory: Record<string, number>;
    uptime_seconds: number;
    schedulers: number;
  };
  backend: Record<string, unknown>;
}

export interface AnalyticsStats {
  events: {
    total: number;
    by_type: Record<string, number>;
    recent_rate: number; // events per minute
  };
  entities: {
    total: number;
    active: number;
  };
  errors: {
    count: number;
    rate: number;
  };
  latency: {
    p50_us: number;
    p99_us: number;
  };
  as_of?: string;
}

export interface EntityTimeline {
  entity_id: string;
  events: TimelineEvent[];
  gaps: TimelineGap[];
  as_of?: string;
}

export interface TimelineEvent {
  id: string;
  event_type: string;
  timestamp: string;
  payload: Record<string, unknown>;
  duration_since_previous_ms?: number;
}

export interface TimelineGap {
  start: string;
  end: string;
  duration_ms: number;
  expected_event_types?: string[];
}

// Team Management types
export interface TeamMember {
  id: string;
  user_id: string;
  email: string;
  name: string;
  role: "owner" | "admin" | "member" | "viewer";
  joined_at: string;
  status: "active" | "pending";
}

export interface TeamMembersResponse {
  members: TeamMember[];
  seat_limit: number;
  seats_used: number;
}

export interface InviteMemberRequest {
  email: string;
  role: "admin" | "member" | "viewer";
}

export interface Invitation {
  id: string;
  email: string;
  role: string;
  invited_by: string;
  invited_at: string;
  status: "pending" | "accepted" | "expired";
}

// Agent key types
export interface AgentKey {
  name: string;
  key_id: string;
  created_at: string;
}

export interface AgentKeyCreated {
  name: string;
  key: string; // raw ask_... value — returned once only
  tenant_id: string;
  created_at: string;
}

export interface AgentKeysResponse {
  agent_keys: AgentKey[];
}

// Audit Log types
export interface AuditLogEntry {
  id: string;
  timestamp: string;
  actor: string;
  action: string;
  details: Record<string, unknown>;
}

export interface AuditLogResponse {
  entries: AuditLogEntry[];
  retention_days: number;
  actions: string[];
}

export interface AuditLogParams {
  action?: string;
  limit?: number;
  offset?: number;
}

// Replay types
export type ReplayStatus = "pending" | "running" | "completed" | "failed" | "cancelled" | "unknown";

export interface ReplayConfig {
  batch_size?: number;
  parallel?: boolean;
  workers?: number;
  emit_progress?: boolean;
  progress_interval?: number;
}

export interface StartReplayRequest {
  from_timestamp?: string;
  to_timestamp?: string;
  event_type?: string;
  entity_id?: string;
  projection_name?: string;
  config?: ReplayConfig;
}

export interface ReplayAnalysisEventType {
  event_type: string;
  count: number;
  share: number;
}

export interface ReplayAnalysisEntity {
  entity_id: string;
  event_count: number;
}

export interface ReplayAnalysisCheck {
  key: string;
  label: string;
  status: "pass" | "warn";
  detail: string;
}

export interface ReplayAnalysis {
  projection_name: string;
  projection_title: string;
  projection_kind: ProjectionKind;
  projection_status: "building" | "ready";
  current_entity_count: number;
  total_events: number;
  sampled_events: number;
  analysis_scope: "full" | "sample";
  event_type_distribution: ReplayAnalysisEventType[];
  sampled_entity_count: number;
  sampled_entities: ReplayAnalysisEntity[];
  first_event_at: string | null;
  last_event_at: string | null;
  analyzed_at: string;
  ready_to_replay: boolean;
  checks: ReplayAnalysisCheck[];
  warnings: string[];
}

export interface ReplayProgress {
  replay_id: string;
  status: ReplayStatus;
  projection_name?: string | null;
  started_at: string;
  updated_at: string;
  completed_at: string | null;
  total_events: number;
  processed_events: number;
  failed_events: number;
  progress_percentage: number;
  events_per_second: number;
  error_message: string | null;
}

export interface ReplayListResponse {
  data: ReplayProgress[];
  total: number;
}

// Usage Analytics types
export interface EventTypeDistribution {
  event_type: string;
  count: number;
}

export interface TopEntity {
  entity_id: string;
  event_count: number;
}

export interface IngestionDataPoint {
  timestamp: string;
  count: number;
}

export interface UsageAnalyticsResponse {
  range: string;
  since: string;
  event_type_distribution: EventTypeDistribution[];
  top_entity_ids: TopEntity[];
  ingestion_rate: IngestionDataPoint[];
  /**
   * Per-tenant daily query series (same shape as `ingestion_rate`). Empty until
   * reads are event-sourced in Core — the dashboard renders an honest empty
   * state in that case, never a fabricated series.
   */
  query_rate: IngestionDataPoint[];
}

// Prime knowledge-graph types — mirror GET /api/v1/prime/graph.
// node ids are `node:<type>:<uuid>`; properties carry human fields
// (name, domain, notes, …). Edges connect node ids by relation.
export interface PrimeGraphNode {
  id: string;
  node_type: string;
  properties: Record<string, unknown>;
  has_vector: boolean;
  vector_dim: number | null;
  created_at: string;
  updated_at: string;
}

export interface PrimeGraphEdge {
  source: string;
  target: string;
  relation: string;
  properties: Record<string, unknown> | null;
  weight: number | null;
  created_at: string;
}

export interface PrimeGraphStats {
  node_count: number;
  edge_count: number;
  vector_count: number;
  nodes_by_type: Record<string, number>;
}

export interface PrimeGraph {
  nodes: PrimeGraphNode[];
  edges: PrimeGraphEdge[];
  stats: PrimeGraphStats;
  has_more: boolean;
}

export interface PrimeGraphParams {
  node_type?: string;
  limit?: number;
}

// Export singleton instance — uses relative URLs for browser requests
export const apiClient = new ApiClient();

/**
 * Absolute URLs for server-side route handlers ONLY.
 * NEVER call these from client components — use the `apiClient` singleton instead,
 * which uses relative URLs to go through the Next.js proxy.
 */
export function getServerApiUrl(): string {
  if (typeof window !== "undefined") {
    throw new Error("getServerApiUrl() must only be called from server-side code");
  }
  return process.env.NEXT_PUBLIC_API_URL || "http://localhost:3902";
}

export function getServerControlPlaneUrl(): string {
  if (typeof window !== "undefined") {
    throw new Error("getServerControlPlaneUrl() must only be called from server-side code");
  }
  return process.env.NEXT_PUBLIC_CONTROL_PLANE_URL || "http://localhost:3901";
}
