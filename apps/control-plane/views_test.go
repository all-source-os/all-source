package main

import (
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

type viewCoreRequest struct {
	method, path, auth, body, ifNoneMatch string
	query                                 map[string][]string
}

type viewCore struct {
	mu         sync.Mutex
	requests   []viewCoreRequest
	increments atomic.Int64
}

func (core *viewCore) last() (request viewCoreRequest, count int) {
	core.mu.Lock()
	defer core.mu.Unlock()
	if len(core.requests) == 0 {
		return viewCoreRequest{}, 0
	}
	return core.requests[len(core.requests)-1], len(core.requests)
}

func newViewCore(t *testing.T) (*viewCore, *httptest.Server) {
	t.Helper()
	core := &viewCore{}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.HasSuffix(r.URL.Path, "/usage/increment") {
			core.increments.Add(1)
			_, _ = w.Write([]byte(`{"type":"queries","used":1}`)) //nolint:errcheck // test response
			return
		}
		body, _ := io.ReadAll(r.Body) //nolint:errcheck // test reader
		core.mu.Lock()
		core.requests = append(core.requests, viewCoreRequest{method: r.Method, path: r.URL.Path, auth: r.Header.Get("Authorization"), body: string(body), ifNoneMatch: r.Header.Get("If-None-Match"), query: r.URL.Query()})
		core.mu.Unlock()
		w.Header().Set("ETag", `"sha256:0123"`)
		if r.Header.Get("If-None-Match") == `"sha256:0123"` {
			w.WriteHeader(http.StatusNotModified)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"view":"ops_status"}`)) //nolint:errcheck // test response
	}))
	t.Cleanup(server.Close)
	return core, server
}

func viewTestRouter(t *testing.T, cp *ControlPlane, role entities.Role, guard bool) *gin.Engine {
	t.Helper()
	router := gin.New()
	router.Use(func(c *gin.Context) {
		c.Set("auth", &AuthContext{UserID: "user-viewer", TenantID: "view-tenant", Role: role})
		c.Set("auth_tenant_id", "view-tenant")
		c.Set("auth_user_id", "user-viewer")
		c.Set("auth_role", role)
		c.Next()
	})
	api := router.Group("/api/v1")
	if guard {
		api.Use(cp.enforceEgress)
	}
	cp.registerViewRoutes(api)
	return router
}

func TestViewRoutesForwardToCoreWithTheAuthenticatedTenant(t *testing.T) {
	for _, tc := range []struct {
		name, method, target, body string
		role                       entities.Role
		status                     int
		path, wait                 string
		meters                     int64
	}{
		{"list", "GET", "/api/v1/views?tenant_id=spoof", "", entities.RoleDeveloper, 200, "/api/v1/views", "", 1},
		{"get", "GET", "/api/v1/views/ops_status?tenant_id=spoof", "", entities.RoleDeveloper, 200, "/api/v1/views/ops_status", "", 1},
		{"define", "PUT", "/api/v1/views/ops_status?tenant_id=spoof", `{"source":{"prefixes":["ops.probe."]}}`, entities.RoleDeveloper, 200, "/api/v1/views/ops_status", "", 0},
		{"delete", "DELETE", "/api/v1/views/ops_status?tenant_id=spoof", "", entities.RoleDeveloper, 200, "/api/v1/views/ops_status", "", 0},
		{"entities", "GET", "/api/v1/views/ops_status/entities?prefix=probe-a&limit=10&tenant_id=spoof", "", entities.RoleDeveloper, 200, "/api/v1/views/ops_status/entities", "", 1},
		{"entity", "GET", "/api/v1/views/ops_status/entities/probe-a:2026-10-27?tenant_id=spoof", "", entities.RoleDeveloper, 200, "/api/v1/views/ops_status/entities/probe-a:2026-10-27", "", 1},
		{"entity escaped once", "GET", "/api/v1/views/ops_status/entities/probe%20a", "", entities.RoleDeveloper, 200, "/api/v1/views/ops_status/entities/probe a", "", 1},
		{"changes", "GET", "/api/v1/views/ops_status/changes?after=61&tenant_id=spoof", "", entities.RoleDeveloper, 200, "/api/v1/views/ops_status/changes", "", 1},
		{"changes wait clamped high", "GET", "/api/v1/views/ops_status/changes?after=61&wait=90", "", entities.RoleDeveloper, 200, "/api/v1/views/ops_status/changes", "20", 1},
		{"changes wait clamped low", "GET", "/api/v1/views/ops_status/changes?after=61&wait=-3", "", entities.RoleDeveloper, 200, "/api/v1/views/ops_status/changes", "0", 1},
		{"invalid name", "GET", "/api/v1/views/Ops.Status", "", entities.RoleDeveloper, 400, "", "", 0},
		{"long name", "GET", "/api/v1/views/" + strings.Repeat("a", 49), "", entities.RoleDeveloper, 400, "", "", 0},
		{"dot key", "GET", "/api/v1/views/ops_status/entities/..", "", entities.RoleDeveloper, 400, "", "", 0},
		{"readonly define", "PUT", "/api/v1/views/ops_status", `{}`, entities.RoleReadOnly, 403, "", "", 0},
		{"readonly delete", "DELETE", "/api/v1/views/ops_status", "", entities.RoleReadOnly, 403, "", "", 0},
		{"admin list never routed", "GET", "/api/v1/admin/views", "", entities.RoleAdmin, 404, "", "", 0},
		{"admin view never routed", "PUT", "/api/v1/admin/views/ops_status", `{}`, entities.RoleAdmin, 404, "", "", 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			core, server := newViewCore(t)
			cp := newTestCP(t, server.URL, server.URL)
			router := viewTestRouter(t, cp, tc.role, false)
			body := io.Reader(http.NoBody)
			if tc.body != "" {
				body = strings.NewReader(tc.body)
			}
			response := httptest.NewRecorder()
			router.ServeHTTP(response, httptest.NewRequestWithContext(t.Context(), tc.method, tc.target, body))
			cp.queryMeter.flush(t.Context())
			if response.Code != tc.status {
				t.Fatalf("status=%d want %d: %s", response.Code, tc.status, response.Body.String())
			}
			forwarded, count := core.last()
			if tc.path == "" {
				if count != 0 {
					t.Fatalf("refused request reached Core: %+v", forwarded)
				}
				return
			}
			if count != 1 || forwarded.method != tc.method || forwarded.path != tc.path {
				t.Fatalf("forwarded %d requests, last %s %q; want %s %q", count, forwarded.method, forwarded.path, tc.method, tc.path)
			}
			if _, spoofed := forwarded.query["tenant_id"]; spoofed {
				t.Fatalf("client tenant_id reached Core: %v", forwarded.query)
			}
			if !strings.Contains(forwarded.auth, ":view-tenant:") {
				t.Fatalf("delegated token does not bind the authenticated tenant: %q", forwarded.auth)
			}
			if tc.wait != "" && strings.Join(forwarded.query["wait"], ",") != tc.wait {
				t.Fatalf("wait=%v want %s", forwarded.query["wait"], tc.wait)
			}
			if forwarded.body != tc.body {
				t.Fatalf("body=%q want %q", forwarded.body, tc.body)
			}
			if got := core.increments.Load(); got != tc.meters {
				t.Fatalf("query usage increments=%d want %d", got, tc.meters)
			}
		})
	}
}

func TestViewDefinitionBodyIsBounded(t *testing.T) {
	core, server := newViewCore(t)
	cp := newTestCP(t, server.URL, server.URL)
	router := viewTestRouter(t, cp, entities.RoleDeveloper, false)
	response := httptest.NewRecorder()
	router.ServeHTTP(response, httptest.NewRequestWithContext(t.Context(), http.MethodPut, "/api/v1/views/ops_status", strings.NewReader(strings.Repeat("x", maxViewDefinition+1))))
	if response.Code != http.StatusRequestEntityTooLarge {
		t.Fatalf("status=%d", response.Code)
	}
	if _, count := core.last(); count != 0 {
		t.Fatal("oversized definition reached Core")
	}
}

func TestEntityStateForcesTenantAndPassesHistoryThrough(t *testing.T) {
	for _, suffix := range []string{"state", "snapshot"} {
		t.Run(suffix, func(t *testing.T) {
			core, server := newViewCore(t)
			cp := newTestCP(t, server.URL, server.URL)
			request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/entities/skill:abc/"+suffix+"?history=true&as_of=2026-10-27T10:00:00Z&tenant_id=spoof", http.NoBody)
			response := callHandler(func(c *gin.Context) {
				c.Params = gin.Params{{Key: "entity_id", Value: "skill:abc"}}
				cp.proxyEntity(c, suffix)
			}, request, "entity-tenant")
			if response.Code != http.StatusOK {
				t.Fatalf("status=%d", response.Code)
			}
			forwarded, _ := core.last()
			if forwarded.path != "/api/v1/entities/skill:abc/"+suffix {
				t.Fatalf("path=%q", forwarded.path)
			}
			for name, want := range map[string]string{"tenant_id": "entity-tenant", "history": "true", "as_of": "2026-10-27T10:00:00Z"} {
				if got := strings.Join(forwarded.query[name], ","); got != want {
					t.Errorf("%s=%q want %q", name, got, want)
				}
			}
		})
	}
}

func TestTenantCannotCommitReservedViewReadModels(t *testing.T) {
	for _, tc := range []struct {
		method, name string
		status       int
		forwarded    bool
	}{
		{http.MethodPut, "view_ops_status", http.StatusForbidden, false},
		{http.MethodPut, "views_registry", http.StatusForbidden, false},
		{http.MethodPut, "board", http.StatusOK, true},
		{http.MethodGet, "view_ops_status", http.StatusOK, true},
	} {
		t.Run(tc.method+" "+tc.name, func(t *testing.T) {
			core, server := newViewCore(t)
			cp := newTestCP(t, server.URL, server.URL)
			request := httptest.NewRequestWithContext(t.Context(), tc.method, "/api/v1/read-models/"+tc.name+"?tenant_id=spoof", strings.NewReader(`{"expected_revision":0,"changes":{}}`))
			response := callHandler(func(c *gin.Context) {
				c.Params = gin.Params{{Key: "name", Value: tc.name}}
				cp.ProxyReadModel(c)
			}, request, "model-tenant")
			cp.queryMeter.flush(t.Context())
			if response.Code != tc.status {
				t.Fatalf("status=%d want %d: %s", response.Code, tc.status, response.Body.String())
			}
			forwarded, count := core.last()
			if (count == 1) != tc.forwarded {
				t.Fatalf("forwarded=%d want %v", count, tc.forwarded)
			}
			if _, spoofed := forwarded.query["tenant_id"]; spoofed {
				t.Fatalf("client tenant_id reached Core: %v", forwarded.query)
			}
		})
	}
}

func TestManagedScopeCoversViewRoutes(t *testing.T) {
	for _, tc := range []struct{ method, path, scope string }{
		{http.MethodGet, "/api/v1/views", "projections:read"},
		{http.MethodGet, "/api/v1/views/ops_status", "projections:read"},
		{http.MethodGet, "/api/v1/views/ops_status/entities", "projections:read"},
		{http.MethodGet, "/api/v1/views/ops_status/entities/probe-a:2026-10-27", "projections:read"},
		{http.MethodGet, "/api/v1/views/ops_status/changes", "projections:read"},
		{http.MethodPut, "/api/v1/views/ops_status", managedProjectionWrite},
		{http.MethodDelete, "/api/v1/views/ops_status", managedProjectionWrite},
		{http.MethodPost, "/api/v1/views/ops_status", ""},
		{http.MethodPut, "/api/v1/views", ""},
		{http.MethodGet, "/api/v1/viewsx", ""},
		{http.MethodGet, "/api/v1/admin/views", ""},
	} {
		t.Run(tc.method+" "+tc.path, func(t *testing.T) {
			c, _ := gin.CreateTestContext(httptest.NewRecorder())
			c.Request = httptest.NewRequestWithContext(t.Context(), tc.method, tc.path, http.NoBody)
			if got := managedScope(c); got != tc.scope {
				t.Fatalf("scope=%q want %q", got, tc.scope)
			}
		})
	}
	if managedRolePermits(entities.RoleReadOnly, managedProjectionWrite) {
		t.Fatal("a readonly key must not define or delete views")
	}
	if !managedRolePermits(entities.RoleReadOnly, "projections:read") {
		t.Fatal("a readonly key must read views")
	}
}

func TestConditionalRoutesMeter304AsZeroBytes(t *testing.T) {
	notModified := func(c *gin.Context) {
		c.Header("ETag", `"sha256:abc"`)
		c.Status(http.StatusNotModified)
		_, _ = c.Writer.WriteString("stale body") //nolint:errcheck // test handler
	}
	routes := []string{"/views", "/views/:view", "/views/:view/entities", "/views/:view/entities/:key", "/views/:view/changes", "/entities/:entity_id/state", "/entities/:entity_id/snapshot", "/read-models/:name", "/event-feed", "/events/query"}
	targets := []string{"/api/v1/views", "/api/v1/views/ops", "/api/v1/views/ops/entities", "/api/v1/views/ops/entities/k1", "/api/v1/views/ops/changes?after=3", "/api/v1/entities/e1/state", "/api/v1/entities/e1/snapshot", "/api/v1/read-models/board", "/api/v1/event-feed?limit=5", "/api/v1/events/query"}
	for _, target := range targets {
		t.Run(target, func(t *testing.T) {
			ledger := &countingLedger{}
			cp := &ControlPlane{egressExchange: ledger.exchange}
			router := gin.New()
			router.Use(func(c *gin.Context) { c.Set("auth_tenant_id", "conditional-tenant"); c.Next() })
			api := router.Group("/api/v1")
			api.Use(cp.enforceEgress)
			for _, route := range routes {
				api.GET(route, notModified)
			}
			response := httptest.NewRecorder()
			router.ServeHTTP(response, httptest.NewRequestWithContext(t.Context(), http.MethodGet, target, http.NoBody))
			if response.Code != http.StatusNotModified || response.Body.Len() != 0 || response.Header().Get("ETag") != `"sha256:abc"` || response.Header().Get("Content-Length") != "" {
				t.Fatalf("304: status=%d body=%q headers=%v", response.Code, response.Body.String(), response.Header())
			}
			if len(ledger.bytes) != 1 || ledger.bytes[0] != 0 {
				t.Fatalf("304 settled bytes %v", ledger.bytes)
			}
		})
	}
}

func TestView304FromCoreReachesClientUnbilled(t *testing.T) {
	core, server := newViewCore(t)
	cp := newTestCP(t, server.URL, server.URL)
	ledger := &countingLedger{}
	cp.egressExchange = ledger.exchange
	router := viewTestRouter(t, cp, entities.RoleDeveloper, true)
	request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/views/ops_status/entities/probe-a:2026-10-27", http.NoBody)
	request.Header.Set("If-None-Match", `"sha256:0123"`)
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	cp.queryMeter.flush(t.Context())
	if response.Code != http.StatusNotModified || response.Header().Get("ETag") != `"sha256:0123"` || response.Body.Len() != 0 {
		t.Fatalf("status=%d etag=%q body=%q", response.Code, response.Header().Get("ETag"), response.Body.String())
	}
	if forwarded, _ := core.last(); forwarded.ifNoneMatch != `"sha256:0123"` {
		t.Fatalf("If-None-Match not forwarded: %q", forwarded.ifNoneMatch)
	}
	if len(ledger.bytes) != 1 || ledger.bytes[0] != 0 || core.increments.Load() != 0 {
		t.Fatalf("304 billed: bytes=%v queries=%d", ledger.bytes, core.increments.Load())
	}
}

func TestViewChangesHoldClassification(t *testing.T) {
	for _, tc := range []struct {
		method, target, route string
		hold                  time.Duration
		longPoll              bool
	}{
		{http.MethodGet, "/api/v1/views/ops/changes?after=1", viewChangesRoute, 0, false},
		{http.MethodGet, "/api/v1/views/ops/changes?wait=0", viewChangesRoute, 0, false},
		{http.MethodGet, "/api/v1/views/ops/changes?wait=soon", viewChangesRoute, 0, false},
		{http.MethodGet, "/api/v1/views/ops/changes?wait=5", viewChangesRoute, 5 * time.Second, true},
		{http.MethodGet, "/api/v1/views/ops/changes?wait=600", viewChangesRoute, 20 * time.Second, true},
		{http.MethodGet, "/api/v1/views/ops/entities?wait=5", viewEntitiesRoute, 0, false},
		{http.MethodPost, "/api/v1/views/ops/changes?wait=5", viewChangesRoute, 0, false},
	} {
		request := httptest.NewRequestWithContext(t.Context(), tc.method, tc.target, http.NoBody)
		if hold, longPoll := viewChangesHold(request, tc.route); hold != tc.hold || longPoll != tc.longPoll {
			t.Errorf("%s %s: hold=%v longPoll=%v want %v %v", tc.method, tc.target, hold, longPoll, tc.hold, tc.longPoll)
		}
	}
}

func TestLongPollLaneCapsEachTenantAndThePool(t *testing.T) {
	lane := newLongPollLane(3, 2)
	for _, step := range []struct {
		tenant string
		want   bool
	}{{"a", true}, {"a", true}, {"a", false}, {"b", true}, {"b", false}} {
		if got := lane.acquire(step.tenant); got != step.want {
			t.Fatalf("acquire(%s)=%v want %v", step.tenant, got, step.want)
		}
	}
	lane.release("a")
	if !lane.acquire("b") {
		t.Fatal("a released slot was not reusable")
	}
	if occupied, held := lane.occupancy("a"); occupied != 3 || held != 1 {
		t.Fatalf("occupied=%d held=%d", occupied, held)
	}
}

func longPollRouter(ledger *countingLedger, admission *tenantAdmission, held <-chan struct{}, entered chan<- time.Time) *gin.Engine {
	router := gin.New()
	cp := &ControlPlane{egressExchange: ledger.exchange, admission: admission}
	router.Use(func(c *gin.Context) { c.Set("auth_tenant_id", c.GetHeader("Test-Tenant")); c.Next() })
	api := router.Group("/api/v1")
	api.Use(cp.enforceEgress)
	api.GET("/views/:view/changes", func(c *gin.Context) {
		deadline, _ := c.Request.Context().Deadline()
		entered <- deadline
		select {
		case <-held:
		case <-c.Request.Context().Done():
		}
		c.String(http.StatusOK, `{"changes":[]}`)
	})
	api.GET("/events/query", func(c *gin.Context) {
		deadline, _ := c.Request.Context().Deadline()
		entered <- deadline
		c.String(http.StatusOK, "ok")
	})
	return router
}

func TestHeldViewChangesNeverBlocksAnOrdinaryRead(t *testing.T) {
	limits := defaultAdmissionLimits()
	limits.InFlight = 1
	ledger := &countingLedger{}
	held := make(chan struct{})
	release := sync.OnceFunc(func() { close(held) })
	defer release()
	entered := make(chan time.Time, 4)
	router := longPollRouter(ledger, newTenantAdmission(admissionEnforce, limits, nil), held, entered)

	done := make(chan *httptest.ResponseRecorder, 1)
	go func() {
		done <- admissionRequestTo(t, router, http.MethodGet, "/api/v1/views/ops/changes?after=1&wait=20", "poller", nil)
	}()
	var pollDeadline time.Time
	select {
	case pollDeadline = <-entered:
	case <-time.After(5 * time.Second):
		t.Fatal("long-poll never reached the handler")
	}
	if remaining := time.Until(pollDeadline); remaining <= 10*time.Second || remaining > 25*time.Second {
		t.Fatalf("long-poll deadline in %v, want wait+5s", remaining)
	}
	if len(egressResponseSlots) != 0 {
		t.Fatalf("held long-poll took %d global response slots", len(egressResponseSlots))
	}
	if occupied, mine := viewChangesLane.occupancy("poller"); occupied != 1 || mine != 1 {
		t.Fatalf("view changes lane occupied=%d held=%d", occupied, mine)
	}

	read := admissionRequest(t, router, "poller", nil)
	if read.Code != http.StatusOK {
		t.Fatalf("ordinary read behind a held long-poll: %d %s", read.Code, read.Body.String())
	}
	if readDeadline := <-entered; time.Until(readDeadline) > 10*time.Second {
		t.Fatalf("ordinary read deadline widened to %v", time.Until(readDeadline))
	}

	release()
	poll := <-done
	if poll.Code != http.StatusOK || poll.Body.String() != `{"changes":[]}` {
		t.Fatalf("long-poll: %d %q", poll.Code, poll.Body.String())
	}
	if occupied, _ := viewChangesLane.occupancy("poller"); occupied != 0 {
		t.Fatalf("long-poll slot leaked: %d", occupied)
	}
	ledger.mu.Lock()
	defer ledger.mu.Unlock()
	billed := map[uint64]bool{}
	for _, bytes := range ledger.bytes {
		billed[bytes] = true
	}
	if len(ledger.bytes) != 2 || !billed[uint64(len(`{"changes":[]}`))] || !billed[uint64(len("ok"))] {
		t.Fatalf("settled bytes %v", ledger.bytes)
	}
}

func TestViewChangesLaneIgnoresAFullGlobalPool(t *testing.T) {
	for range cap(egressResponseSlots) {
		egressResponseSlots <- struct{}{}
	}
	defer func() {
		for range cap(egressResponseSlots) {
			<-egressResponseSlots
		}
	}()
	held := make(chan struct{})
	close(held)
	entered := make(chan time.Time, 4)
	router := longPollRouter(&countingLedger{}, nil, held, entered)
	if poll := admissionRequestTo(t, router, http.MethodGet, "/api/v1/views/ops/changes?after=1&wait=5", "poller", nil); poll.Code != http.StatusOK {
		t.Fatalf("long-poll refused by the global pool: %d", poll.Code)
	}
	if plain := admissionRequestTo(t, router, http.MethodGet, "/api/v1/views/ops/changes?after=1", "poller", nil); plain.Code != http.StatusTooManyRequests {
		t.Fatalf("a wait=0 changes read must take the global pool: %d", plain.Code)
	}
	if read := admissionRequest(t, router, "poller", nil); read.Code != http.StatusTooManyRequests {
		t.Fatalf("ordinary read with a full pool: %d", read.Code)
	}
}

func TestViewChangesLaneRefusesPastTheTenantCap(t *testing.T) {
	for range viewChangesLane.perTenant {
		if !viewChangesLane.acquire("greedy") {
			t.Fatal("lane full before the per-tenant cap")
		}
	}
	defer func() {
		for range viewChangesLane.perTenant {
			viewChangesLane.release("greedy")
		}
	}()
	held := make(chan struct{})
	close(held)
	ledger := &countingLedger{}
	router := longPollRouter(ledger, nil, held, make(chan time.Time, 4))
	refused := admissionRequestTo(t, router, http.MethodGet, "/api/v1/views/ops/changes?after=1&wait=5", "greedy", nil)
	if refused.Code != http.StatusTooManyRequests || refused.Header().Get("Retry-After") == "" {
		t.Fatalf("over-cap long-poll: %d retry=%q", refused.Code, refused.Header().Get("Retry-After"))
	}
	if ledger.total() != 0 {
		t.Fatal("refused long-poll reached the ledger")
	}
	if other := admissionRequestTo(t, router, http.MethodGet, "/api/v1/views/ops/changes?after=1&wait=5", "neighbor", nil); other.Code != http.StatusOK {
		t.Fatalf("another tenant's long-poll: %d", other.Code)
	}
}
