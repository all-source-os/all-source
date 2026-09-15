package main

// Data-plane delegation. Control Plane is the single public entry point for
// chronis and the SDKs. After AuthMiddleware validates the caller (JWT or
// ask_ API key), these handlers forward the request to the correct internal
// backend — writes to Core, reads to Query Service — with the authenticated
// tenant_id injected from the caller's identity. The forwarded request is
// authenticated with Control Plane's service JWT, so backends never see
// public auth material and never need to validate ask_ keys themselves.

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strings"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

// delegationClient holds the resolved backend URLs and the shared HTTP
// client used to forward requests. signToken mints a per-request JWT for
// the authenticated caller — NOT a Control Plane admin token — so backends
// see the real tenant and role and enforce isolation accordingly.
//
// Regional routing (Step 2 of REGIONAL_INDEPENDENCE.md): when
// coreByRegion has entries AND homeRegion is wired, writes route to
// the tenant's home-region Core via coreForTenant; otherwise the
// `core` field is the single target (current single-region default).
// The single-region fast path skips the tenant lookup entirely so
// there is zero overhead until multi-region followers exist.
type delegationClient struct {
	core         *url.URL
	coreByRegion map[string]*url.URL
	queryService *url.URL
	prime        *url.URL
	// homeRegion resolves a tenant ID to its home region. Wired by
	// NewControlPlane to a closure over the tenant repository. nil
	// means "single-region mode" — coreForTenant returns the default
	// core unconditionally.
	homeRegion func(tenantID string) string
	signToken  func(userID, tenantID string, role entities.Role) (string, error)
	http       *http.Client
}

func newDelegationClient(coreURL, queryURL, primeURL string, signToken func(userID, tenantID string, role entities.Role) (string, error), httpClient *http.Client) (*delegationClient, error) {
	if signToken == nil {
		return nil, errors.New("delegation requires signToken")
	}
	core, err := url.Parse(strings.TrimRight(coreURL, "/"))
	if err != nil {
		return nil, fmt.Errorf("parse core URL: %w", err)
	}
	qs, err := url.Parse(strings.TrimRight(queryURL, "/"))
	if err != nil {
		return nil, fmt.Errorf("parse query-service URL: %w", err)
	}
	prime, err := url.Parse(strings.TrimRight(primeURL, "/"))
	if err != nil {
		return nil, fmt.Errorf("parse prime URL: %w", err)
	}
	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	return &delegationClient{
		core:         core,
		coreByRegion: make(map[string]*url.URL),
		queryService: qs,
		prime:        prime,
		signToken:    signToken,
		http:         httpClient,
	}, nil
}

// addRegionalCore registers a regional Core URL. After all calls, the
// map is consulted by coreForTenant; an unknown region falls back to
// the default core. Returns an error if url is unparseable.
func (d *delegationClient) addRegionalCore(region, rawURL string) error {
	u, err := url.Parse(strings.TrimRight(rawURL, "/"))
	if err != nil {
		return fmt.Errorf("parse regional core URL %q: %w", region, err)
	}
	d.coreByRegion[region] = u
	return nil
}

// setHomeRegionResolver wires a tenant-id → region resolver. Pass nil
// to revert to single-region mode (coreForTenant returns the default
// core regardless of tenant). The resolver may return an empty
// string or an unknown region; coreForTenant treats both as "fall
// back to default."
func (d *delegationClient) setHomeRegionResolver(f func(tenantID string) string) {
	d.homeRegion = f
}

// coreForTenant returns the Core URL that owns writes for tenantID.
// Single-region fast path (no regional cores configured OR no
// resolver wired): returns the default core unconditionally with no
// tenant lookup. Multi-region path: resolves the tenant's home
// region via the wired resolver and returns the matching URL,
// falling back to the default core on misses.
//
// The fast path means the routing change is a no-op until an
// operator actually configures a second region — no risk of
// regressing single-region performance just by landing this code.
func (d *delegationClient) coreForTenant(tenantID string) *url.URL {
	if len(d.coreByRegion) == 0 || d.homeRegion == nil {
		return d.core
	}
	region := d.homeRegion(tenantID)
	if region == "" {
		return d.core
	}
	if u, ok := d.coreByRegion[region]; ok {
		return u
	}
	return d.core
}

// authTenantFromContext returns the authenticated caller's tenant_id set by
// AuthMiddleware. Delegation requires a tenant — unauthenticated or
// tenantless callers cannot reach these routes. Returns false when absent
// so handlers can 401 rather than forwarding an un-scoped write.
func authTenantFromContext(c *gin.Context) (string, bool) {
	v, ok := c.Get("auth_tenant_id")
	if !ok {
		return "", false
	}
	s, ok := v.(string)
	if !ok || s == "" {
		return "", false
	}
	return s, true
}

// authIdentityFromContext returns the three identity fields delegation needs
// to mint a per-request JWT: user ID, tenant ID, and role. AuthMiddleware
// sets these after validating either a JWT or an ask_ API key, so a
// successful context lookup means the caller is authenticated.
func authIdentityFromContext(c *gin.Context) (userID, tenantID string, role entities.Role, ok bool) {
	tenantID, ok = authTenantFromContext(c)
	if !ok {
		return
	}
	uid, uidOk := c.Get("auth_user_id")
	r, rOk := c.Get("auth_role")
	if !uidOk || !rOk {
		ok = false
		return
	}
	uidStr, uidStrOk := uid.(string)
	if !uidStrOk {
		ok = false
		return
	}
	roleVal, roleOk := r.(entities.Role)
	if !roleOk {
		ok = false
		return
	}
	return uidStr, tenantID, roleVal, true
}

// injectTenantIntoObject overwrites the top-level tenant_id field in a JSON
// object. Used for single-event ingest where the Core handler reads
// req.tenant_id.
func injectTenantIntoObject(body []byte, tenantID string) ([]byte, error) {
	var doc map[string]any
	if len(body) == 0 {
		doc = map[string]any{}
	} else if err := json.Unmarshal(body, &doc); err != nil {
		return nil, fmt.Errorf("decode ingest body: %w", err)
	}
	doc["tenant_id"] = tenantID
	return json.Marshal(doc)
}

// injectTenantIntoBatch overwrites tenant_id on every element in a batch
// request's "events" array. Callers that submit a pre-populated tenant per
// event still get overwritten — trusting the gateway is the whole point.
func injectTenantIntoBatch(body []byte, tenantID string) ([]byte, error) {
	var doc map[string]any
	if err := json.Unmarshal(body, &doc); err != nil {
		return nil, fmt.Errorf("decode batch body: %w", err)
	}
	events, ok := doc["events"].([]any)
	if !ok {
		return nil, errors.New("batch body missing 'events' array")
	}
	for i, raw := range events {
		ev, ok := raw.(map[string]any)
		if !ok {
			return nil, fmt.Errorf("events[%d] is not a JSON object", i)
		}
		ev["tenant_id"] = tenantID
		events[i] = ev
	}
	doc["events"] = events
	return json.Marshal(doc)
}

// forwardRequest builds a new outgoing request against backend, copies the
// body, attaches the per-request bearer JWT (minted from the authenticated
// caller's identity), and copies the backend's response to the client. It
// does NOT forward the inbound Authorization header — the backend trusts
// the JWT we signed, not any ask_ key the client presented.
func (d *delegationClient) forwardRequest(c *gin.Context, method string, backend *url.URL, path string, query url.Values, body []byte, bearerToken string, extraHeaders map[string]string) {
	target := *backend
	target.Path = strings.TrimRight(backend.Path, "/") + "/" + strings.TrimLeft(path, "/")
	if query != nil {
		target.RawQuery = query.Encode()
	}

	var bodyReader io.Reader
	if body != nil {
		bodyReader = bytes.NewReader(body)
	}

	req, err := http.NewRequestWithContext(c.Request.Context(), method, target.String(), bodyReader)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "delegation build request", "message": err.Error()})
		return
	}
	req.Header.Set("Authorization", "Bearer "+bearerToken)
	for k, v := range extraHeaders {
		req.Header.Set(k, v)
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}

	resp, err := d.http.Do(req)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": "delegation upstream", "message": err.Error()})
		return
	}
	defer func() { _ = resp.Body.Close() }() //nolint:errcheck // close-on-defer, non-actionable

	// Copy status + response body. Content-Type comes from the backend so
	// JSON responses carry through unchanged.
	if ct := resp.Header.Get("Content-Type"); ct != "" {
		c.Writer.Header().Set("Content-Type", ct)
	}
	c.Writer.WriteHeader(resp.StatusCode)
	if _, err := io.Copy(c.Writer, resp.Body); err != nil {
		// Response headers already sent — best we can do is log.
		_ = err
	}
}

// mintBearer builds a per-request JWT that represents the authenticated
// caller. Backends validate this JWT against the shared JWT_SECRET and get
// the right tenant + role — which is how tenant isolation survives the hop
// through Control Plane.
func (cp *ControlPlane) mintBearer(c *gin.Context) (string, bool) {
	userID, tenantID, role, ok := authIdentityFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "unauthorized", "message": "no auth context"})
		return "", false
	}
	token, err := cp.delegation.signToken(userID, tenantID, role)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "delegation sign", "message": err.Error()})
		return "", false
	}
	return token, true
}

// proxyIngest is the shared core of ProxyIngestSingle and ProxyIngestBatch —
// they differ only in the tenant-injection function and the upstream path.
func (cp *ControlPlane) proxyIngest(
	c *gin.Context,
	upstreamPath string,
	inject func(body []byte, tenantID string) ([]byte, error),
) {
	_, tenantID, _, ok := authIdentityFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "unauthorized", "message": "no auth context"})
		return
	}
	body, err := io.ReadAll(c.Request.Body)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad request", "message": err.Error()})
		return
	}
	rewritten, err := inject(body, tenantID)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad request", "message": err.Error()})
		return
	}
	bearer, ok := cp.mintBearer(c)
	if !ok {
		return
	}
	cp.delegation.forwardRequest(c, http.MethodPost, cp.delegation.coreForTenant(tenantID), upstreamPath, nil, rewritten, bearer, nil)
}

// ProxyIngestSingle forwards POST /api/v1/events to Core with tenant_id
// injected into the JSON body from the authenticated caller.
func (cp *ControlPlane) ProxyIngestSingle(c *gin.Context) {
	cp.proxyIngest(c, "/api/v1/events", injectTenantIntoObject)
}

// ProxyIngestBatch forwards POST /api/v1/events/batch to Core with tenant_id
// injected onto every event in the batch.
func (cp *ControlPlane) ProxyIngestBatch(c *gin.Context) {
	cp.proxyIngest(c, "/api/v1/events/batch", injectTenantIntoBatch)
}

// ProxyPrime forwards every /api/v1/prime/* request (any method) to the Prime
// MCP service on the internal network. Prime's own data model is per-node
// (graph, vectors, recall) — tenant scoping lives inside Prime, not in the
// gateway. We pass tenant_id as a query param so Prime can key its namespace
// off it, and attach the per-request JWT so Prime can independently verify
// the caller's identity if it chooses to.
func (cp *ControlPlane) ProxyPrime(c *gin.Context) {
	_, tenantID, _, ok := authIdentityFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "unauthorized", "message": "no auth context"})
		return
	}

	// Gin captures the suffix as "*path" — strip the leading slash Gin adds.
	suffix := strings.TrimPrefix(c.Param("path"), "/")
	downstreamPath := "/api/v1/prime/" + suffix
	// allsource-prime serves MCP JSON-RPC at its root, not under /api/v1/prime,
	// so the hosted MCP URL is api.all-source.xyz/api/v1/prime/mcp.
	if suffix == "mcp" {
		downstreamPath = "/mcp"
	}

	q := c.Request.URL.Query()
	q.Set("tenant_id", tenantID)

	var body []byte
	if c.Request.Body != nil {
		var err error
		body, err = io.ReadAll(c.Request.Body)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "bad request", "message": err.Error()})
			return
		}
	}

	// The stateless allsource-prime app reads the tenant from the X-Tenant-Id
	// HEADER (not a query param) and gates its hosted surface behind
	// PRIME_API_KEY. When that shared secret is configured here, authenticate
	// to the app with it and stamp the trusted tenant header. If it is NOT set
	// (pre-migration / local), fall back to the user-scoped service JWT + the
	// query param so nothing breaks before the app is switched to hosted mode.
	extraHeaders := map[string]string{"X-Tenant-Id": tenantID}
	var bearer string
	if key := os.Getenv("PRIME_API_KEY"); key != "" {
		bearer = key
	} else {
		var ok bool
		bearer, ok = cp.mintBearer(c)
		if !ok {
			return
		}
	}
	cp.delegation.forwardRequest(c, c.Request.Method, cp.delegation.prime, downstreamPath, q, body, bearer, extraHeaders)
}

// forwardExtraction POSTs the OpenAI-compatible request body to the server-side
// extraction LLM at the exact upstream URL (no path joining — the configured URL
// is the full chat-completions endpoint), authenticating with AllSource's own
// provider key. That key is NEVER returned to the caller: only the LLM's JSON
// response (choices + usage) is copied back. Mirrors forwardRequest's status +
// body copy so streaming and non-streaming replies both pass through.
func (d *delegationClient) forwardExtraction(c *gin.Context, upstreamURL, apiKey string, body []byte) {
	// upstreamURL is EXTRACTION_LLM_URL from the process environment (see the
	// single caller, ProxyExtraction) — operator-configured, never caller-derived.
	// Only the request BODY comes from the tenant. G704's taint analysis cannot
	// see that, so it flags the parameter.
	req, err := http.NewRequestWithContext(c.Request.Context(), http.MethodPost, upstreamURL, bytes.NewReader(body)) //nolint:gosec // G704 false positive: URL is operator-set env, not user input
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "extraction build request", "message": err.Error()})
		return
	}
	req.Header.Set("Content-Type", "application/json")
	if apiKey != "" {
		req.Header.Set("Authorization", "Bearer "+apiKey)
	}

	resp, err := d.http.Do(req) //nolint:gosec // G704 false positive: see above
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": "extraction upstream", "message": err.Error()})
		return
	}
	defer func() { _ = resp.Body.Close() }() //nolint:errcheck // close-on-defer, non-actionable

	if ct := resp.Header.Get("Content-Type"); ct != "" {
		c.Writer.Header().Set("Content-Type", ct)
	}
	c.Writer.WriteHeader(resp.StatusCode)
	if _, err := io.Copy(c.Writer, resp.Body); err != nil {
		// Response headers already sent — best we can do is drop it.
		_ = err
	}
}

// ProxyExtraction is the hosted Hound doc-extraction LLM proxy and the
// enforcement point for the extraction-token HARD GATE (t-57fca0). A tenant
// running prime-mcp in hosted mode points PRIME_LLM_ENDPOINT at this route and
// PRIME_LLM_API_KEY at its own ask_ key, so the OpenAI-compatible
// chat-completions request arrives authenticated. The handler:
//
//  1. resolves the tenant from the ask_ key (AuthMiddleware),
//  2. HARD-GATES on HasExtractionQuota — a tenant past its per-tier extraction
//     allowance gets 402 and the request never reaches an LLM (no spend, no
//     charge — pure enforcement, per the chosen "hard-gate when over"),
//  3. forwards the body to AllSource's server-side LLM (EXTRACTION_LLM_URL +
//     EXTRACTION_LLM_API_KEY, never exposed to the tenant) and copies the
//     OpenAI response back verbatim.
//
// prime-mcp parses choices+usage from that response and emits
// prime.extraction.usage events; the billing reconciler meters them, which feeds
// the next HasExtractionQuota decision. The proxy deliberately does NOT emit
// usage — metering stays single-sourced in prime-mcp to avoid double counting.
//
// When EXTRACTION_LLM_URL is unset the route 503s: hosted extraction is opt-in
// infrastructure. Tenants can always bring their own LLM by pointing
// PRIME_LLM_ENDPOINT at their own OpenAI-compatible endpoint instead, which
// never touches this proxy or the gate.
func (cp *ControlPlane) ProxyExtraction(c *gin.Context) {
	_, tenantID, _, ok := authIdentityFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "unauthorized", "message": "no auth context"})
		return
	}

	// HARD GATE — the whole point of the route. Over-allowance tenants are
	// blocked before any LLM spend. HasExtractionQuota fails open on a Core blip
	// so a transient outage never wrongly blocks a paying tenant.
	if cp.extractionGate != nil && !cp.extractionGate.HasExtractionQuota(tenantID) {
		c.JSON(http.StatusPaymentRequired, gin.H{
			"error":   "extraction_quota_exceeded",
			"message": "hosted Hound extraction tokens are exhausted for this billing period. Upgrade your plan, or set PRIME_LLM_ENDPOINT to your own OpenAI-compatible endpoint to bring your own LLM.",
		})
		return
	}

	upstream := strings.TrimSpace(os.Getenv("EXTRACTION_LLM_URL"))
	if upstream == "" {
		c.JSON(http.StatusServiceUnavailable, gin.H{
			"error":   "hosted_extraction_unconfigured",
			"message": "hosted extraction LLM is not configured on this deployment. Set PRIME_LLM_ENDPOINT to your own OpenAI-compatible endpoint (BYO).",
		})
		return
	}

	body, err := io.ReadAll(c.Request.Body)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad request", "message": err.Error()})
		return
	}

	cp.delegation.forwardExtraction(c, upstream, os.Getenv("EXTRACTION_LLM_API_KEY"), body)
}

// ProxyEventsQuery forwards GET /api/v1/events/query to Core with tenant_id
// injected as a query param AND a per-request JWT scoped to the caller's
// tenant + role. Core's query_events enforces tenant from auth, which with
// the user-scoped JWT matches what we're asking for. Any client-supplied
// tenant_id in the URL is overwritten — the authenticated caller's tenant
// is authoritative.
// ProxyStats serves GET /api/v1/stats tenant-scoped.
//
// Core's /api/v1/stats is global (store.stats()) unless a tenant_id is supplied,
// so this MUST force the auth-derived tenant — proxying it bare would hand the
// caller whole-fleet totals. Forcing it also means a caller-supplied tenant_id is
// overwritten, not honored (#230).
func (cp *ControlPlane) ProxyStats(c *gin.Context) {
	_, tenantID, _, ok := authIdentityFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "unauthorized", "message": "no auth context"})
		return
	}
	q := c.Request.URL.Query()
	q.Set("tenant_id", tenantID)
	bearer, ok := cp.mintBearer(c)
	if !ok {
		return
	}
	cp.delegation.forwardRequest(c, http.MethodGet, cp.delegation.coreForTenant(tenantID), "/api/v1/stats", q, nil, bearer, nil)
}

// ProxyEntityState serves GET /api/v1/entities/:entity_id/state tenant-scoped.
//
// Core reconstructs from tenant-filtered events (and skips the untenanted
// snapshot fast path) when tenant_id is present, so forcing it here is what
// keeps one tenant from reading another's entity by id (#230).
func (cp *ControlPlane) ProxyEntityState(c *gin.Context) {
	cp.proxyEntity(c, "state")
}

// ProxyEntitySnapshot serves GET /api/v1/entities/:entity_id/snapshot
// tenant-scoped. Same reasoning as ProxyEntityState.
func (cp *ControlPlane) ProxyEntitySnapshot(c *gin.Context) {
	cp.proxyEntity(c, "snapshot")
}

func (cp *ControlPlane) proxyEntity(c *gin.Context, suffix string) {
	_, tenantID, _, ok := authIdentityFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "unauthorized", "message": "no auth context"})
		return
	}
	entityID := c.Param("entity_id")
	if entityID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "bad_request", "message": "entity_id is required"})
		return
	}
	q := c.Request.URL.Query()
	q.Set("tenant_id", tenantID)
	bearer, ok := cp.mintBearer(c)
	if !ok {
		return
	}
	path := "/api/v1/entities/" + url.PathEscape(entityID) + "/" + suffix
	cp.delegation.forwardRequest(c, http.MethodGet, cp.delegation.coreForTenant(tenantID), path, q, nil, bearer, nil)
}

func (cp *ControlPlane) ProxyEventsQuery(c *gin.Context) {
	_, tenantID, _, ok := authIdentityFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "unauthorized", "message": "no auth context"})
		return
	}
	q := c.Request.URL.Query()
	q.Set("tenant_id", tenantID)
	bearer, ok := cp.mintBearer(c)
	if !ok {
		return
	}
	// Reads currently route to the same core as writes. Once
	// cross-region read replicas exist (Step 3 of
	// REGIONAL_INDEPENDENCE.md), this should switch to
	// coreForRegion(localRegion) so reads stay in-region. For now
	// "home region" resolves to a single Core for both directions.
	cp.delegation.forwardRequest(c, http.MethodGet, cp.delegation.coreForTenant(tenantID), "/api/v1/events/query", q, nil, bearer, nil)
}
