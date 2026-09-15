package main

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"testing"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func init() {
	gin.SetMode(gin.TestMode)
}

// fakeBackend records the incoming request so tests can assert tenant
// injection and auth forwarding behavior.
type fakeBackend struct {
	method string
	path   string
	query  url.Values
	auth   string
	tenant string
	body   []byte
	respBy func(w http.ResponseWriter, r *http.Request)
}

func newFakeBackend(respBy func(w http.ResponseWriter, r *http.Request)) (*fakeBackend, *httptest.Server) {
	fb := &fakeBackend{respBy: respBy}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body) //nolint:errcheck // test reader
		fb.method = r.Method
		fb.path = r.URL.Path
		fb.query = r.URL.Query()
		fb.auth = r.Header.Get("Authorization")
		fb.tenant = r.Header.Get("X-Tenant-Id")
		fb.body = body
		fb.respBy(w, r)
	}))
	return fb, srv
}

// newTestCP builds a minimal ControlPlane with only the pieces the delegation
// handlers use. It skips AuthMiddleware; tests call handlers directly with a
// pre-populated gin.Context to isolate delegation behavior.
//
// The token signer is deterministic for test assertions — it echoes the
// inputs so tests can read tenant/user/role out of the forwarded Authorization
// header without JWT parsing.
func newTestCP(t *testing.T, coreURL, qsURL string) *ControlPlane {
	t.Helper()
	signer := func(userID, tenantID string, role entities.Role) (string, error) {
		return "test-token:" + userID + ":" + tenantID + ":" + string(role), nil
	}
	d, err := newDelegationClient(coreURL, qsURL, qsURL, signer, http.DefaultClient)
	if err != nil {
		t.Fatalf("newDelegationClient: %v", err)
	}
	return &ControlPlane{delegation: d}
}

func callHandler(h gin.HandlerFunc, req *http.Request, tenantID string) *httptest.ResponseRecorder {
	w := httptest.NewRecorder()
	c, _ := gin.CreateTestContext(w)
	c.Request = req
	if tenantID != "" {
		c.Set("auth_tenant_id", tenantID)
		c.Set("auth_user_id", "user-"+tenantID)
		c.Set("auth_role", entities.RoleDeveloper)
	}
	h(c)
	return w
}

func TestProxyIngestSingle_InjectsTenantAndForwardsToCore(t *testing.T) {
	core, srv := newFakeBackend(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"event_id":"11111111-1111-1111-1111-111111111111","version":1}`)) //nolint:errcheck // test response
	})
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	body := bytes.NewBufferString(`{"event_type":"order.created","entity_id":"o-1","payload":{"amount":10},"tenant_id":"spoofed"}`)
	req := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/api/v1/events", body)
	w := callHandler(cp.ProxyIngestSingle, req, "tenant-real")

	if w.Code != http.StatusOK {
		t.Fatalf("status: got %d, want 200 (body=%s)", w.Code, w.Body.String())
	}
	if core.path != "/api/v1/events" {
		t.Errorf("upstream path: got %q, want /api/v1/events", core.path)
	}
	if core.auth != "Bearer test-token:user-tenant-real:tenant-real:developer" {
		t.Errorf("upstream auth: got %q, want per-request JWT scoped to caller", core.auth)
	}

	var forwarded map[string]any
	if err := json.Unmarshal(core.body, &forwarded); err != nil {
		t.Fatalf("decode forwarded body: %v", err)
	}
	if forwarded["tenant_id"] != "tenant-real" {
		t.Errorf("tenant_id: got %v, want tenant-real (spoofed client value must be overwritten)", forwarded["tenant_id"])
	}
	if forwarded["event_type"] != "order.created" {
		t.Errorf("event_type not forwarded: got %v", forwarded["event_type"])
	}
	if forwarded["entity_id"] != "o-1" {
		t.Errorf("entity_id not forwarded: got %v", forwarded["entity_id"])
	}
}

func TestProxyIngestSingle_NoTenantContext401(t *testing.T) {
	cp := newTestCP(t, "http://unused", "http://unused")
	req := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/api/v1/events", strings.NewReader(`{}`))
	w := callHandler(cp.ProxyIngestSingle, req, "")
	if w.Code != http.StatusUnauthorized {
		t.Errorf("status: got %d, want 401", w.Code)
	}
}

func TestProxyIngestBatch_InjectsTenantOnEveryEvent(t *testing.T) {
	core, srv := newFakeBackend(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"ingested":2}`)) //nolint:errcheck // test response
	})
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	batch := `{"events":[
		{"event_type":"a","entity_id":"e1","payload":{},"tenant_id":"spoof-a"},
		{"event_type":"b","entity_id":"e2","payload":{}}
	]}`
	req := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/api/v1/events/batch", strings.NewReader(batch))
	w := callHandler(cp.ProxyIngestBatch, req, "tenant-real")

	if w.Code != http.StatusOK {
		t.Fatalf("status: got %d, want 200 (body=%s)", w.Code, w.Body.String())
	}
	if core.path != "/api/v1/events/batch" {
		t.Errorf("upstream path: got %q, want /api/v1/events/batch", core.path)
	}
	var forwarded struct {
		Events []map[string]any `json:"events"`
	}
	if err := json.Unmarshal(core.body, &forwarded); err != nil {
		t.Fatalf("decode forwarded batch: %v", err)
	}
	if len(forwarded.Events) != 2 {
		t.Fatalf("events count: got %d, want 2", len(forwarded.Events))
	}
	for i, ev := range forwarded.Events {
		if ev["tenant_id"] != "tenant-real" {
			t.Errorf("events[%d].tenant_id: got %v, want tenant-real", i, ev["tenant_id"])
		}
	}
}

func TestProxyEventsQuery_InjectsTenantAsQueryParam(t *testing.T) {
	qs, srv := newFakeBackend(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"events":[],"count":0}`)) //nolint:errcheck // test response
	})
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	// Client supplies a spoofed tenant_id plus legitimate since= filter.
	// Spoofed tenant must be overwritten; since= must be forwarded unchanged.
	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/events/query?tenant_id=spoof&since=2026-04-17T00:00:00Z", http.NoBody)
	w := callHandler(cp.ProxyEventsQuery, req, "tenant-real")

	if w.Code != http.StatusOK {
		t.Fatalf("status: got %d, want 200 (body=%s)", w.Code, w.Body.String())
	}
	if qs.path != "/api/v1/events/query" {
		t.Errorf("upstream path: got %q, want /api/v1/events/query", qs.path)
	}
	if got := qs.query.Get("tenant_id"); got != "tenant-real" {
		t.Errorf("upstream tenant_id: got %q, want tenant-real", got)
	}
	if got := qs.query.Get("since"); got != "2026-04-17T00:00:00Z" {
		t.Errorf("upstream since: got %q, want original value", got)
	}
	if qs.auth != "Bearer test-token:user-tenant-real:tenant-real:developer" {
		t.Errorf("upstream auth: got %q, want per-request JWT scoped to caller", qs.auth)
	}
}

func TestProxyEventsQuery_NoTenantContext401(t *testing.T) {
	cp := newTestCP(t, "http://unused", "http://unused")
	req := httptest.NewRequestWithContext(context.Background(), http.MethodGet, "/api/v1/events/query", http.NoBody)
	w := callHandler(cp.ProxyEventsQuery, req, "")
	if w.Code != http.StatusUnauthorized {
		t.Errorf("status: got %d, want 401", w.Code)
	}
}

func TestProxyPrime_CatchAllForwardsWithTenantAndJWT(t *testing.T) {
	primeHit, srv := newFakeBackend(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"nodes":[]}`)) //nolint:errcheck // test response
	})
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	// Simulate Gin's "*path" param by setting the context param directly.
	body := strings.NewReader(`{"query":"match (n) return n"}`)
	req := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/api/v1/prime/shortest-path", body)
	w := httptest.NewRecorder()
	c, _ := gin.CreateTestContext(w)
	c.Request = req
	c.Set("auth_tenant_id", "tenant-real")
	c.Set("auth_user_id", "user-x")
	c.Set("auth_role", entities.RoleDeveloper)
	c.Params = gin.Params{{Key: "path", Value: "/shortest-path"}}

	cp.ProxyPrime(c)

	if w.Code != http.StatusOK {
		t.Fatalf("status: got %d, want 200 (body=%s)", w.Code, w.Body.String())
	}
	if primeHit.path != "/api/v1/prime/shortest-path" {
		t.Errorf("upstream path: got %q, want /api/v1/prime/shortest-path", primeHit.path)
	}
	if primeHit.method != "POST" {
		t.Errorf("upstream method: got %q, want POST", primeHit.method)
	}
	if got := primeHit.query.Get("tenant_id"); got != "tenant-real" {
		t.Errorf("upstream tenant_id: got %q, want tenant-real", got)
	}
	if primeHit.auth == "" || !strings.HasPrefix(primeHit.auth, "Bearer test-token:") {
		t.Errorf("upstream auth: got %q, want per-request JWT", primeHit.auth)
	}
	if len(primeHit.body) == 0 {
		t.Error("upstream body was empty — POST body should forward")
	}
}

// The hosted MCP transport: a tenant's API key authenticates at the gateway,
// and allsource-prime trusts only the X-Tenant-Id the gateway stamps. A caller
// that sends its own X-Tenant-Id must not reach another tenant's graph.
func TestProxyPrime_MCPForwardsToPrimeRootWithGatewayTenant(t *testing.T) {
	t.Setenv("PRIME_API_KEY", "prime-shared-secret")
	primeHit, srv := newFakeBackend(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}`)) //nolint:errcheck // test response
	})
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	body := strings.NewReader(`{"jsonrpc":"2.0","id":1,"method":"tools/list"}`)
	req := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/api/v1/prime/mcp", body)
	req.Header.Set("X-Tenant-Id", "tenant-victim")
	req.Header.Set("Authorization", "Bearer ask_tenant_real_key")
	w := httptest.NewRecorder()
	c, _ := gin.CreateTestContext(w)
	c.Request = req
	c.Set("auth_tenant_id", "tenant-real")
	c.Set("auth_user_id", "user-x")
	c.Set("auth_role", entities.RoleServiceAccount)
	c.Params = gin.Params{{Key: "path", Value: "/mcp"}}

	cp.ProxyPrime(c)

	if w.Code != http.StatusOK {
		t.Fatalf("status: got %d, want 200 (body=%s)", w.Code, w.Body.String())
	}
	if primeHit.path != "/mcp" {
		t.Errorf("upstream path: got %q, want /mcp", primeHit.path)
	}
	if primeHit.tenant != "tenant-real" {
		t.Errorf("upstream X-Tenant-Id: got %q, want tenant-real (caller-supplied header must not pass through)", primeHit.tenant)
	}
	if primeHit.auth != "Bearer prime-shared-secret" {
		t.Errorf("upstream auth: got %q, want the gateway's PRIME_API_KEY, never the tenant key", primeHit.auth)
	}
	if !strings.Contains(string(primeHit.body), `"tools/list"`) {
		t.Errorf("upstream body: got %q, want the JSON-RPC request unchanged", primeHit.body)
	}
	if !strings.Contains(w.Body.String(), `"result"`) {
		t.Errorf("response: got %q, want the JSON-RPC reply copied back", w.Body.String())
	}
}

func TestDelegation_UpstreamErrorPropagates(t *testing.T) {
	_, srv := newFakeBackend(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusConflict)
		_, _ = w.Write([]byte(`{"error":"version conflict"}`)) //nolint:errcheck // test response
	})
	defer srv.Close()
	cp := newTestCP(t, srv.URL, srv.URL)

	req := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/api/v1/events", strings.NewReader(`{"event_type":"x","entity_id":"e","payload":{}}`))
	w := callHandler(cp.ProxyIngestSingle, req, "tenant-real")

	if w.Code != http.StatusConflict {
		t.Errorf("status: got %d, want 409 propagated from upstream", w.Code)
	}
	if !strings.Contains(w.Body.String(), "version conflict") {
		t.Errorf("body: got %q, want upstream error to pass through", w.Body.String())
	}
}

// Claims import guard — without this, the entities/jwt packages look unused
// to the checker when other tests in this file are compiled in isolation.
var _ = entities.RoleDeveloper
var _ = jwt.SigningMethodHS256

// Step 2 of REGIONAL_INDEPENDENCE.md: coreForTenant routing. The
// tests below cover the contract — single-region fast path,
// multi-region routing by tenant home_region, and graceful fallback
// to the default Core when the resolver returns "" or an unknown
// region. They use the delegationClient directly (no HTTP) since
// only the routing decision is under test.

func newTestDelegation(t *testing.T) *delegationClient {
	t.Helper()
	d, err := newDelegationClient(
		"http://core.iad.test",
		"http://qs.test",
		"http://prime.test",
		func(_, _ string, _ entities.Role) (string, error) { return "tok", nil },
		http.DefaultClient,
	)
	if err != nil {
		t.Fatalf("newDelegationClient: %v", err)
	}
	return d
}

func TestDelegationClient_CoreForTenant_SingleRegionFastPath(t *testing.T) {
	// Default config: no regional Cores registered, no resolver.
	// Every tenant returns the default core, no resolver call.
	d := newTestDelegation(t)

	got := d.coreForTenant("acme")
	if got.String() != "http://core.iad.test" {
		t.Errorf("coreForTenant returned %q, want default core", got)
	}
}

func TestDelegationClient_CoreForTenant_RoutesByRegion(t *testing.T) {
	d := newTestDelegation(t)
	if err := d.addRegionalCore("iad", "http://core.iad.test"); err != nil {
		t.Fatal(err)
	}
	if err := d.addRegionalCore("lhr", "http://core.lhr.test"); err != nil {
		t.Fatal(err)
	}

	// Resolver: tenants are pinned by ID prefix in this test.
	d.setHomeRegionResolver(func(tenantID string) string {
		switch {
		case strings.HasPrefix(tenantID, "lhr-"):
			return "lhr"
		case strings.HasPrefix(tenantID, "iad-"):
			return "iad"
		default:
			return ""
		}
	})

	if got := d.coreForTenant("lhr-acme").String(); got != "http://core.lhr.test" {
		t.Errorf("lhr-acme routed to %q, want core.lhr.test", got)
	}
	if got := d.coreForTenant("iad-acme").String(); got != "http://core.iad.test" {
		t.Errorf("iad-acme routed to %q, want core.iad.test", got)
	}
}

func TestDelegationClient_CoreForTenant_FallsBackOnUnknownRegion(t *testing.T) {
	d := newTestDelegation(t)
	if err := d.addRegionalCore("lhr", "http://core.lhr.test"); err != nil {
		t.Fatal(err)
	}
	d.setHomeRegionResolver(func(_ string) string { return "atlantis" })

	got := d.coreForTenant("acme")
	if got.String() != "http://core.iad.test" {
		t.Errorf("unknown region routed to %q, want default core fallback", got)
	}
}

func TestDelegationClient_CoreForTenant_FallsBackOnEmptyResolver(t *testing.T) {
	d := newTestDelegation(t)
	if err := d.addRegionalCore("lhr", "http://core.lhr.test"); err != nil {
		t.Fatal(err)
	}
	// Resolver returns "" — same path as a tenant with no
	// home_region in metadata (legacy rows pre-migration).
	d.setHomeRegionResolver(func(_ string) string { return "" })

	got := d.coreForTenant("legacy-tenant")
	if got.String() != "http://core.iad.test" {
		t.Errorf("empty region routed to %q, want default core", got)
	}
}

func TestDelegationClient_CoreForTenant_NoResolverIsSingleRegion(t *testing.T) {
	// Regional Cores registered but no resolver wired → single-region
	// behavior preserved. Useful for migrations where the URLs are
	// added before the resolver is hooked up.
	d := newTestDelegation(t)
	if err := d.addRegionalCore("lhr", "http://core.lhr.test"); err != nil {
		t.Fatal(err)
	}

	got := d.coreForTenant("any-tenant")
	if got.String() != "http://core.iad.test" {
		t.Errorf("no resolver routed to %q, want default core", got)
	}
}
