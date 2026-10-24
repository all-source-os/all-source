package main

import (
	"math"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
)

func TestAdmissionLaneIgnoresClientAssertions(t *testing.T) {
	for _, tc := range []struct {
		method, target, route string
		header                http.Header
		want                  string
	}{
		{http.MethodGet, "/api/v1/events/query", "/api/v1/events/query", nil, admissionLaneData},
		{http.MethodGet, "/api/v1/events/query", "/api/v1/events/query", http.Header{"If-None-Match": {`"bogus"`}}, admissionLaneData},
		{http.MethodGet, "/api/v1/events/query?wait=30", "/api/v1/events/query", nil, admissionLaneData},
		{http.MethodGet, "/api/v1/prime/heads", "/api/v1/prime/*path", nil, admissionLaneData},
		{http.MethodGet, "/api/v1/prime/graph/heads", "/api/v1/prime/*path", nil, admissionLaneData},
		{http.MethodGet, "/api/v1/read-models/heads", "/api/v1/read-models/:name", nil, admissionLaneData},
		{http.MethodGet, "/api/v1/unrouted/heads", "", nil, admissionLaneData},
		{http.MethodPost, "/api/v1/events", "/api/v1/events", nil, admissionLaneWrite},
		{http.MethodPut, "/api/v1/read-models/board", "/api/v1/read-models/:name", nil, admissionLaneWrite},
	} {
		request := httptest.NewRequestWithContext(t.Context(), tc.method, tc.target, http.NoBody)
		for name, values := range tc.header {
			request.Header[name] = values
		}
		if got := admissionLane(request, tc.route); got != tc.want {
			t.Errorf("%s %s %v: lane %s want %s", tc.method, tc.target, tc.header, got, tc.want)
		}
	}
}

func TestTenantChosenHeadsPathDoesNotSkipTheByteBucket(t *testing.T) {
	limits := defaultAdmissionLimits()
	limits.BytesBurst, limits.BytesPerSecond = 10, 0.001
	admission := newTenantAdmission(admissionEnforce, limits, nil)
	cp := &ControlPlane{egressExchange: (&countingLedger{}).exchange, admission: admission}
	router := gin.New()
	router.Use(func(c *gin.Context) { c.Set("auth_tenant_id", c.GetHeader("Test-Tenant")); c.Next() })
	api := router.Group("/api/v1")
	api.Use(cp.enforceEgress)
	body := func(c *gin.Context) { c.String(200, strings.Repeat("x", 15)) }
	api.GET("/read-models/:name", body)
	api.Any("/prime/*path", body)
	for _, target := range []string{"/api/v1/read-models/heads", "/api/v1/prime/graph/heads"} {
		tenant := "heads-" + target
		if first := admissionRequestTo(t, router, http.MethodGet, target, tenant, nil); first.Code != 200 {
			t.Fatalf("%s first read: %d", target, first.Code)
		}
		second := admissionRequestTo(t, router, http.MethodGet, target, tenant, nil)
		if body := refusalBody(t, second); second.Code != http.StatusTooManyRequests || body["lane"] != admissionLaneData {
			t.Fatalf("%s with the byte bucket in deficit: %d %v", target, second.Code, body)
		}
		if meta := admission.tenants[tenant].meta.tokens; meta != limits.MetaBurst {
			t.Fatalf("%s charged the meta lane: %v", target, meta)
		}
	}
}

func laneLimits() admissionLimits {
	limits := tightLimits()
	limits.DataBurst, limits.MetaBurst, limits.BytesBurst, limits.InFlight = 5, 5, 1000, 4
	limits.WritePerSecond, limits.WriteBurst = 0.001, 1
	return limits
}

func assertTokens(t *testing.T, name string, got, want float64) {
	t.Helper()
	if math.Abs(got-want) > 0.05 {
		t.Fatalf("%s tokens=%v want %v", name, got, want)
	}
}

func TestBogusETagAnswered200IsChargedAsData(t *testing.T) {
	for _, mode := range []admissionMode{admissionShadow, admissionEnforce} {
		t.Run(string(mode), func(t *testing.T) {
			admission := newTenantAdmission(mode, laneLimits(), nil)
			router := admissionTestRouter((&countingLedger{}).exchange, admission, func(c *gin.Context) { c.String(200, "abcdef") })
			response := admissionRequestTo(t, router, http.MethodGet, "/api/v1/events/query?wait=30", "bogus-etag", http.Header{"If-None-Match": {`"bogus"`}})
			if response.Code != 200 || response.Body.String() != "abcdef" {
				t.Fatalf("read: %d %s", response.Code, response.Body.String())
			}
			state := admission.tenants["bogus-etag"]
			assertTokens(t, "data", state.data.tokens, 4)
			assertTokens(t, "meta", state.meta.tokens, 5)
			assertTokens(t, "bytes", state.bytes.tokens, 994)
		})
	}
}

func TestServer304IsRefundedIntoTheMetaLane(t *testing.T) {
	admission := newTenantAdmission(admissionEnforce, laneLimits(), nil)
	router := admissionTestRouter((&countingLedger{}).exchange, admission, func(c *gin.Context) { c.Status(http.StatusNotModified) })
	if response := admissionRequest(t, router, "real-304", http.Header{"If-None-Match": {`"v1"`}}); response.Code != http.StatusNotModified {
		t.Fatalf("conditional read: %d", response.Code)
	}
	state := admission.tenants["real-304"]
	assertTokens(t, "data", state.data.tokens, 5)
	assertTokens(t, "meta", state.meta.tokens, 4)
	assertTokens(t, "bytes", state.bytes.tokens, 1000)
	for range 5 {
		admissionRequest(t, router, "real-304", nil)
	}
	response := admissionRequest(t, router, "real-304", nil)
	if body := refusalBody(t, response); response.Code != http.StatusTooManyRequests || body["lane"] != admissionLaneMeta {
		t.Fatalf("304s beyond the meta bucket were not refused: %d %v", response.Code, body)
	}
}

func TestWritesBypassGlobalPoolAndLedger(t *testing.T) {
	occupyGlobalPool(t)
	for name, admission := range admissionModes() {
		t.Run(name, func(t *testing.T) {
			ledger := &countingLedger{}
			router := admissionTestRouter(ledger.exchange, admission, func(c *gin.Context) { c.String(http.StatusCreated, `{"ok":true}`) })
			response := admissionRequestTo(t, router, http.MethodPost, "/api/v1/events", "writer-"+name, nil)
			if response.Code != http.StatusCreated {
				t.Fatalf("write refused with the pool full: %d %s", response.Code, response.Body.String())
			}
			if read := admissionRequest(t, router, "writer-"+name, nil); read.Code < 400 {
				t.Fatalf("read admitted with the pool full: %d", read.Code)
			}
			if ledger.total() != 0 {
				t.Fatalf("write or refused read reached the ledger: %d calls", ledger.total())
			}
		})
	}
	for _, tc := range []struct {
		method, target string
		billable       bool
	}{
		{http.MethodPost, "/api/v1/events", false},
		{http.MethodPost, "/api/v1/events/batch", false},
		{http.MethodGet, "/api/v1/events/query", true},
	} {
		request := httptest.NewRequestWithContext(t.Context(), tc.method, tc.target, http.NoBody)
		if got := billableEgressRequest(request); got != tc.billable {
			t.Fatalf("%s %s billable=%v want %v", tc.method, tc.target, got, tc.billable)
		}
	}
}

func TestEmptyFeedProbeSkipsPoolButIsMeteredAndReleased(t *testing.T) {
	occupyGlobalPool(t)
	for _, mode := range []admissionMode{admissionShadow, admissionEnforce} {
		t.Run(string(mode), func(t *testing.T) {
			ledger := &countingLedger{}
			admission := newTenantAdmission(mode, laneLimits(), nil)
			cp := &ControlPlane{egressExchange: ledger.exchange, admission: admission}
			router := gin.New()
			router.Use(func(c *gin.Context) { c.Set("auth_tenant_id", c.GetHeader("Test-Tenant")); c.Next() })
			router.GET(eventFeedPath, cp.enforceEgress, func(c *gin.Context) { c.String(200, "{}") })
			for range 3 {
				if response := admissionRequestTo(t, router, http.MethodGet, eventFeedPath+"?limit=0", "probe-"+string(mode), nil); response.Code != 200 {
					t.Fatalf("probe refused: %d %s", response.Code, response.Body.String())
				}
			}
			state := admission.tenants["probe-"+string(mode)]
			if state.inFlight != 0 {
				t.Fatalf("probe leaked an in-flight slot: %d", state.inFlight)
			}
			assertTokens(t, "bytes", state.bytes.tokens, 994)
			if ledger.total() != 0 {
				t.Fatal("probe reached the ledger")
			}
		})
	}
}

func TestPanickingProbeReleasesItsInFlightSlot(t *testing.T) {
	admission := newTenantAdmission(admissionShadow, laneLimits(), nil)
	cp := &ControlPlane{egressExchange: (&countingLedger{}).exchange, admission: admission}
	router := gin.New()
	router.Use(gin.CustomRecovery(func(c *gin.Context, _ any) { c.AbortWithStatus(http.StatusInternalServerError) }))
	router.Use(func(c *gin.Context) { c.Set("auth_tenant_id", c.GetHeader("Test-Tenant")); c.Next() })
	router.GET(eventFeedPath, cp.enforceEgress, func(*gin.Context) { panic("probe handler") })
	if response := admissionRequestTo(t, router, http.MethodGet, eventFeedPath+"?limit=0", "probe-panic", nil); response.Code != http.StatusInternalServerError {
		t.Fatalf("panicking probe: %d", response.Code)
	}
	if inFlight := admission.tenants["probe-panic"].inFlight; inFlight != 0 {
		t.Fatalf("panicking probe leaked an in-flight slot: %d", inFlight)
	}
}

func TestBillableWriteIsADataLaneRead(t *testing.T) {
	request := httptest.NewRequestWithContext(t.Context(), http.MethodPost, "/api/v1/projections/board/query", http.NoBody)
	if got := admissionLane(request, "/api/v1/projections/:name/query"); got != admissionLaneData {
		t.Fatalf("billable POST lane %s", got)
	}
}

func TestEnforcedWriteLaneRefusesWithoutLedger(t *testing.T) {
	ledger := &countingLedger{}
	router := admissionTestRouter(ledger.exchange, newTenantAdmission(admissionEnforce, laneLimits(), nil), func(c *gin.Context) { c.Status(http.StatusCreated) })
	if first := admissionRequestTo(t, router, http.MethodPost, "/api/v1/events", "write-heavy", nil); first.Code != http.StatusCreated {
		t.Fatalf("first write: %d", first.Code)
	}
	second := admissionRequestTo(t, router, http.MethodPost, "/api/v1/events", "write-heavy", nil)
	if body := refusalBody(t, second); second.Code != http.StatusTooManyRequests || body["lane"] != admissionLaneWrite || body["scope"] != admissionScopeTenant {
		t.Fatalf("write over burst: %d %v", second.Code, body)
	}
	if ledger.total() != 0 {
		t.Fatal("write reached the ledger")
	}
	shadow := admissionTestRouter(ledger.exchange, newTenantAdmission(admissionShadow, laneLimits(), nil), func(c *gin.Context) { c.Status(http.StatusCreated) })
	for range 3 {
		if response := admissionRequestTo(t, shadow, http.MethodPost, "/api/v1/events", "write-shadow", nil); response.Code != http.StatusCreated {
			t.Fatalf("shadow refused a write: %d", response.Code)
		}
	}
}
