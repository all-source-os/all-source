package main

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/prometheus/client_golang/prometheus"
)

type countingLedger struct {
	mu    sync.Mutex
	calls map[string]int
	bytes []uint64
}

func (l *countingLedger) exchange(_ context.Context, tenant string, command egressCommand) (egressAdmission, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.calls == nil {
		l.calls = map[string]int{}
	}
	l.calls[tenant]++
	if command.Operation == egressOperationComplete {
		l.bytes = append(l.bytes, command.Bytes)
	}
	var result egressAdmission
	result.Protocol = fundedEgressProtocol
	result.Admission.ReservedBytes = maxEgressResponse
	return result, nil
}

func (l *countingLedger) callsFor(tenant string) int {
	l.mu.Lock()
	defer l.mu.Unlock()
	return l.calls[tenant]
}

func (l *countingLedger) total() int {
	l.mu.Lock()
	defer l.mu.Unlock()
	total := 0
	for _, calls := range l.calls {
		total += calls
	}
	return total
}

func admissionTestRouter(exchange egressExchange, admission *tenantAdmission, handler gin.HandlerFunc) *gin.Engine {
	router := gin.New()
	cp := &ControlPlane{egressExchange: exchange, admission: admission}
	router.Use(func(c *gin.Context) { c.Set("auth_tenant_id", c.GetHeader("Test-Tenant")); c.Next() })
	api := router.Group("/api/v1")
	api.Use(cp.enforceEgress)
	api.GET("/events/query", handler)
	api.POST("/events", handler)
	return router
}

func admissionRequest(t *testing.T, router http.Handler, tenant string, header http.Header) *httptest.ResponseRecorder {
	t.Helper()
	return admissionRequestTo(t, router, http.MethodGet, "/api/v1/events/query", tenant, header)
}

func admissionRequestTo(t *testing.T, router http.Handler, method, target, tenant string, header http.Header) *httptest.ResponseRecorder {
	t.Helper()
	body := io.Reader(http.NoBody)
	if method == http.MethodPost {
		body = strings.NewReader(`{"event_type":"t.created"}`)
	}
	request := httptest.NewRequestWithContext(t.Context(), method, target, body)
	for name, values := range header {
		request.Header[name] = values
	}
	request.Header.Set("Test-Tenant", tenant)
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	return response
}

func tightLimits() admissionLimits {
	return admissionLimits{DataPerSecond: 0.001, DataBurst: 2, MetaPerSecond: 0.001, MetaBurst: 2, BytesPerSecond: 1, BytesBurst: 4, InFlight: 1}
}

func refusalBody(t *testing.T, response *httptest.ResponseRecorder) map[string]any {
	t.Helper()
	var body map[string]any
	if err := json.Unmarshal(response.Body.Bytes(), &body); err != nil {
		t.Fatalf("refusal body is not JSON: %s", response.Body.String())
	}
	return body
}

func counterValue(t *testing.T, collector prometheus.Collector, labels map[string]string) float64 {
	t.Helper()
	registry := prometheus.NewRegistry()
	registry.MustRegister(collector)
	families, err := registry.Gather()
	if err != nil {
		t.Fatal(err)
	}
	total := 0.0
	for _, family := range families {
		for _, metric := range family.GetMetric() {
			matched := 0
			for _, label := range metric.GetLabel() {
				if want, ok := labels[label.GetName()]; ok && want == label.GetValue() {
					matched++
				}
			}
			if matched == len(labels) {
				total += metric.GetCounter().GetValue()
			}
		}
	}
	return total
}

func TestAdmissionShadowChangesNoBehaviour(t *testing.T) {
	type outcome struct {
		status int
		body   string
	}
	for _, tc := range []struct {
		name    string
		tenants []string
		header  http.Header
		handler gin.HandlerFunc
	}{
		{"data lane over burst", []string{"shadow-a", "shadow-a", "shadow-a", "shadow-a", "shadow-b"}, nil, func(c *gin.Context) { c.String(200, "abcdef") }},
		{"meta lane conditional", []string{"shadow-c", "shadow-c", "shadow-c", "shadow-c"}, http.Header{"If-None-Match": {`"v1"`}}, func(c *gin.Context) { c.Status(http.StatusNotModified) }},
		{"upstream error", []string{"shadow-d", "shadow-d", "shadow-d"}, nil, func(c *gin.Context) { c.String(400, "bad query") }},
	} {
		t.Run(tc.name, func(t *testing.T) {
			run := func(admission *tenantAdmission) []outcome {
				ledger := &countingLedger{}
				router := admissionTestRouter(ledger.exchange, admission, tc.handler)
				outcomes := make([]outcome, 0, len(tc.tenants))
				for _, tenant := range tc.tenants {
					response := admissionRequest(t, router, tenant, tc.header)
					if response.Header().Get("RateLimit-Policy") != "" || response.Header().Get("RateLimit") != "" {
						t.Fatalf("shadow response carries RateLimit headers: %v", response.Header())
					}
					outcomes = append(outcomes, outcome{response.Code, response.Body.String()})
				}
				return outcomes
			}
			shadow := newTenantAdmission(admissionShadow, tightLimits(), nil)
			without, with := run(nil), run(shadow)
			for i := range without {
				if without[i] != with[i] {
					t.Fatalf("request %d changed under shadow: %+v vs %+v", i, without[i], with[i])
				}
			}
			counts, ok := shadow.snapshot(tc.tenants[0], time.Now())["would_refuse"].(map[string]uint64)
			if !ok {
				t.Fatal("snapshot would_refuse has the wrong type")
			}
			if counts[admissionScopeTenant] == 0 {
				t.Fatalf("shadow recorded no would-refuse for %s: %v", tc.tenants[0], counts)
			}
			lane := admissionLaneData
			if tc.header != nil {
				lane = admissionLaneMeta
			}
			if got := counterValue(t, admissionWouldRefuse, map[string]string{"lane": lane, "scope": admissionScopeTenant, "tenant": tc.tenants[0]}); got == 0 {
				t.Fatal("admission_would_refuse_total not populated per tenant")
			}
		})
	}
}

func TestAdmissionEnforceRefusesHeavyTenantBeforeLedger(t *testing.T) {
	ledger := &countingLedger{}
	router := admissionTestRouter(ledger.exchange, newTenantAdmission(admissionEnforce, defaultAdmissionLimits(), nil), func(c *gin.Context) { c.String(200, "ok") })
	admittedHeavy, refusedHeavy, lightOK := 0, 0, 0
	retries := map[int]bool{}
	for i := range 200 {
		response := admissionRequest(t, router, "enforce-heavy", nil)
		switch response.Code {
		case http.StatusOK:
			admittedHeavy++
		case http.StatusTooManyRequests:
			refusedHeavy++
			body := refusalBody(t, response)
			if body["scope"] != admissionScopeTenant || body["lane"] != admissionLaneData {
				t.Fatalf("refusal body lacks scope/lane: %v", body)
			}
			retry, err := strconv.Atoi(response.Header().Get("Retry-After"))
			if err != nil || retry < 1 || retry > maxRetryAfter || retry == 30 {
				t.Fatalf("Retry-After=%q", response.Header().Get("Retry-After"))
			}
			retries[retry] = true
		default:
			t.Fatalf("unexpected status %d", response.Code)
		}
		if i%40 == 0 {
			if admissionRequest(t, router, "enforce-light", nil).Code == http.StatusOK {
				lightOK++
			}
		}
	}
	if refusedHeavy < 150 || admittedHeavy > 25 {
		t.Fatalf("heavy tenant admitted=%d refused=%d", admittedHeavy, refusedHeavy)
	}
	if got := ledger.callsFor("enforce-heavy"); got != 2*admittedHeavy {
		t.Fatalf("refused requests reached the ledger: calls=%d admitted=%d", got, admittedHeavy)
	}
	if lightOK != 5 {
		t.Fatalf("light tenant got %d/5 2xx", lightOK)
	}
	if len(retries) < 2 {
		t.Fatalf("every refused client got the same Retry-After: %v", retries)
	}
}

func TestAdmissionByteBucketRetryAfterFollowsDeficit(t *testing.T) {
	restore := admissionJitter
	admissionJitter = func() int { return 0 }
	t.Cleanup(func() { admissionJitter = restore })
	limits := defaultAdmissionLimits()
	limits.BytesBurst, limits.BytesPerSecond = 10, 1
	ledger := &countingLedger{}
	admission := newTenantAdmission(admissionEnforce, limits, nil)
	router := admissionTestRouter(ledger.exchange, admission, func(c *gin.Context) { c.String(200, strings.Repeat("x", 15)) })
	if response := admissionRequest(t, router, "bytes-tenant", nil); response.Code != 200 {
		t.Fatalf("first read refused: %d", response.Code)
	}
	response := admissionRequest(t, router, "bytes-tenant", nil)
	if response.Code != http.StatusTooManyRequests || response.Header().Get("Retry-After") != "6" {
		t.Fatalf("byte deficit 5 at 1 B/s: status=%d Retry-After=%q", response.Code, response.Header().Get("Retry-After"))
	}
	if response := admissionRequest(t, router, "bytes-tenant", http.Header{"If-None-Match": {`"v"`}}); response.Code != http.StatusTooManyRequests {
		t.Fatalf("If-None-Match bypassed the byte bucket: %d", response.Code)
	}
	ticket, refusal, _ := admission.admit("bytes-tenant", admissionLaneMeta, 0, 16, time.Now())
	if refusal != nil {
		t.Fatalf("meta lane must not wait on the byte bucket: %+v", refusal)
	}
	ticket.release(0, http.StatusOK)
}

func TestRateLimitHeadersOnlyWhenEnforcing(t *testing.T) {
	for name, admission := range admissionModes() {
		t.Run(name, func(t *testing.T) {
			router := admissionTestRouter((&countingLedger{}).exchange, admission, func(c *gin.Context) { c.String(200, "ok") })
			for _, response := range []*httptest.ResponseRecorder{
				admissionRequest(t, router, "headers-"+name, nil),
				admissionRequestTo(t, router, http.MethodPost, "/api/v1/events", "headers-"+name, nil),
			} {
				present := response.Header().Get("RateLimit") != "" || response.Header().Get("RateLimit-Policy") != ""
				if present != admission.enforcing() {
					t.Fatalf("%s: RateLimit headers present=%v: %v", name, present, response.Header())
				}
			}
		})
	}
}

func TestRetryAfterIsComputedAndClamped(t *testing.T) {
	restore := admissionJitter
	t.Cleanup(func() { admissionJitter = restore })
	for _, jitter := range []int{0, 1} {
		admissionJitter = func() int { return jitter }
		for _, tc := range []struct {
			deficit, rate float64
			want          int
		}{
			{1, 10, 1 + jitter},
			{5, 1, 5 + jitter},
			{0.5, 0.01, 50 + jitter},
			{0, 1, 1},
			{1e9, 1, 60},
			{1, 0, 60},
		} {
			if got := retryAfterSeconds(tc.deficit, tc.rate); got != max(1, min(60, tc.want)) {
				t.Fatalf("retryAfterSeconds(%v,%v) jitter=%d = %d want %d", tc.deficit, tc.rate, jitter, got, tc.want)
			}
		}
	}
	admissionJitter = restore
	for range 200 {
		if retry := retryAfterSeconds(1, 1); retry < 1 || retry > 2 {
			t.Fatalf("retry=%d", retry)
		}
	}
}

func TestAdmissionFairShareInsideGlobalPool(t *testing.T) {
	limits := defaultAdmissionLimits()
	limits.InFlight, limits.DataBurst = 16, 100
	for _, tc := range []struct {
		name     string
		mode     admissionMode
		held     int
		occupied int
		refused  bool
	}{
		{"pool busy, tenant at share", admissionEnforce, 4, 12, false},
		{"pool busy, tenant over share", admissionEnforce, 5, 12, true},
		{"pool calm, tenant over share", admissionEnforce, 5, 11, false},
		{"shadow only counts", admissionShadow, 5, 12, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			admission := newTenantAdmission(tc.mode, limits, nil)
			now := time.Now()
			for range tc.held {
				if _, refusal, _ := admission.admit("fair", admissionLaneData, 0, 16, now); refusal != nil {
					t.Fatalf("setup refused: %+v", refusal)
				}
			}
			_, refusal, _ := admission.admit("fair", admissionLaneData, tc.occupied, 16, now)
			if (refusal != nil) != tc.refused {
				t.Fatalf("refusal=%+v want refused=%v", refusal, tc.refused)
			}
			if refusal != nil && (refusal.scope != admissionScopeFairShare || refusal.status != http.StatusTooManyRequests) {
				t.Fatalf("wrong refusal: %+v", refusal)
			}
			wantCount := uint64(0)
			if tc.held == 5 && tc.occupied == 12 {
				wantCount = 1
			}
			if got := admission.tenants["fair"].wouldRefuse[admissionScopeFairShare]; got != wantCount {
				t.Fatalf("fair-share would-refuse=%d want %d", got, wantCount)
			}
		})
	}
}

func occupyGlobalPool(t *testing.T) {
	t.Helper()
	for range cap(egressResponseSlots) {
		egressResponseSlots <- struct{}{}
	}
	t.Cleanup(func() {
		for range cap(egressResponseSlots) {
			<-egressResponseSlots
		}
	})
}

const legacyEgressBody = `{"error":"egress_budget_unavailable","message":"Included allowance and prepaid credit must cover this response. Inspect billing usage or purchase prepaid credit."}`

func admissionModes() map[string]*tenantAdmission {
	return map[string]*tenantAdmission{
		"nil":     nil,
		"off":     newTenantAdmission(admissionOff, defaultAdmissionLimits(), nil),
		"shadow":  newTenantAdmission(admissionShadow, defaultAdmissionLimits(), nil),
		"enforce": newTenantAdmission(admissionEnforce, defaultAdmissionLimits(), nil),
	}
}

func TestGlobalPoolFullKeepsLegacy429InEveryMode(t *testing.T) {
	occupyGlobalPool(t)
	for name, admission := range admissionModes() {
		t.Run(name, func(t *testing.T) {
			ledger := &countingLedger{}
			router := admissionTestRouter(ledger.exchange, admission, func(c *gin.Context) { c.String(200, "ok") })
			response := admissionRequest(t, router, "global-full-"+name, nil)
			if ledger.total() != 0 {
				t.Fatal("full pool reached the ledger")
			}
			if response.Code != http.StatusTooManyRequests || response.Header().Get("Retry-After") != "30" || response.Body.String() != legacyEgressBody {
				t.Fatalf("full pool in %s changed the response: %d Retry-After=%q %s", name, response.Code, response.Header().Get("Retry-After"), response.Body.String())
			}
		})
	}
}

func TestLedgerRefusalKeepsLegacyRetryAfterInEveryMode(t *testing.T) {
	for name, admission := range admissionModes() {
		t.Run(name, func(t *testing.T) {
			for _, status := range []int{http.StatusServiceUnavailable, http.StatusTooManyRequests} {
				router := admissionTestRouter(func(context.Context, string, egressCommand) (egressAdmission, error) {
					return egressAdmission{}, egressError{status: status}
				}, admission, func(c *gin.Context) { c.String(200, "never") })
				response := admissionRequest(t, router, "ledger-down-"+name, nil)
				if response.Code != status || response.Header().Get("Retry-After") != "30" || response.Body.String() != legacyEgressBody {
					t.Fatalf("ledger %d in %s changed the response: %d Retry-After=%q %s", status, name, response.Code, response.Header().Get("Retry-After"), response.Body.String())
				}
			}
		})
	}
}

func TestAdmissionConfigFromEnv(t *testing.T) {
	for _, tc := range []struct {
		name    string
		env     map[string]string
		mode    admissionMode
		wantErr string
	}{
		{"default is shadow", map[string]string{}, admissionShadow, ""},
		{"off", map[string]string{admissionModeEnv: "off"}, admissionOff, ""},
		{"enforce without overrides fails", map[string]string{admissionModeEnv: "enforce"}, "", "requires " + admissionOverridesEnv},
		{"enforce with blank overrides fails", map[string]string{admissionModeEnv: "enforce", admissionOverridesEnv: " "}, "", "requires " + admissionOverridesEnv},
		{"enforce with empty map", map[string]string{admissionModeEnv: "enforce", admissionOverridesEnv: "{}"}, admissionEnforce, ""},
		{"enforce with unparseable overrides fails", map[string]string{admissionModeEnv: "enforce", admissionOverridesEnv: "{"}, "", "parse"},
		{"shadow with unparseable overrides fails", map[string]string{admissionOverridesEnv: "[]"}, "", "parse"},
		{"invalid override fails", map[string]string{admissionOverridesEnv: `{"t":{"data_per_second":-1}}`}, "", "data_per_second"},
		{"unknown mode fails", map[string]string{admissionModeEnv: "on"}, "", "off, shadow or enforce"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			admission, err := newTenantAdmissionFromEnv(func(key string) (string, bool) { v, ok := tc.env[key]; return v, ok })
			if tc.wantErr != "" {
				if err == nil || !strings.Contains(err.Error(), tc.wantErr) {
					t.Fatalf("err=%v want %q", err, tc.wantErr)
				}
				return
			}
			if err != nil || admission.mode != tc.mode {
				t.Fatalf("admission=%+v err=%v", admission, err)
			}
		})
	}
	admission, err := newTenantAdmissionFromEnv(func(key string) (string, bool) {
		return map[string]string{admissionModeEnv: "enforce", admissionOverridesEnv: `{"big":{"data_per_second":60,"data_burst":120}}`}[key], true
	})
	if err != nil {
		t.Fatal(err)
	}
	limits, override := admission.limitsFor("big")
	defaults := defaultAdmissionLimits()
	if !override || limits.DataPerSecond != 60 || limits.DataBurst != 120 || limits.MetaPerSecond != defaults.MetaPerSecond || limits.InFlight != defaults.InFlight {
		t.Fatalf("partial override not merged over defaults: %+v", limits)
	}
}

func TestTenantLimitsReportsCallerStateWithoutCore(t *testing.T) {
	overrides := map[string]admissionLimits{"limits-big": {DataPerSecond: 0.5, DataBurst: 120, MetaPerSecond: 100, MetaBurst: 200, BytesPerSecond: 1 << 20, BytesBurst: maxEgressResponse, InFlight: 12}}
	admission := newTenantAdmission(admissionShadow, defaultAdmissionLimits(), overrides)
	admission.admit("limits-big", admissionLaneData, 0, 16, time.Now())
	cp := &ControlPlane{admission: admission}
	if billableEgressPath(tenantLimitsPath) {
		t.Fatal("limits route must not pass through the ledger")
	}
	request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, tenantLimitsPath, http.NoBody)
	response := callHandler(cp.TenantLimits, request, "limits-big")
	var body struct {
		Mode     admissionMode         `json:"mode"`
		Override bool                  `json:"override"`
		Limits   admissionLimits       `json:"limits"`
		Buckets  map[string]bucketView `json:"buckets"`
		InFlight int                   `json:"in_flight"`
		Global   struct {
			Slots int `json:"slots"`
		} `json:"global"`
	}
	if err := json.Unmarshal(response.Body.Bytes(), &body); err != nil || response.Code != 200 {
		t.Fatalf("limits: %d %s", response.Code, response.Body.String())
	}
	if body.Mode != admissionShadow || !body.Override || body.Limits.DataPerSecond != 0.5 || body.InFlight != 1 || body.Global.Slots != 16 {
		t.Fatalf("limits body: %+v", body)
	}
	if data := body.Buckets["data"]; data.Capacity != 120 || data.Tokens >= 120 {
		t.Fatalf("data bucket not the caller's live state: %+v", data)
	}
	other := callHandler(cp.TenantLimits, httptest.NewRequestWithContext(t.Context(), http.MethodGet, tenantLimitsPath, http.NoBody), "limits-small")
	if !strings.Contains(other.Body.String(), `"override":false`) || strings.Contains(other.Body.String(), "limits-big") {
		t.Fatalf("other tenant saw foreign limits: %s", other.Body.String())
	}
	if _, tracked := admission.tenants["limits-small"]; tracked {
		t.Fatal("reading limits created admission state")
	}
	if callHandler(cp.TenantLimits, httptest.NewRequestWithContext(t.Context(), http.MethodGet, tenantLimitsPath, http.NoBody), "").Code != http.StatusUnauthorized {
		t.Fatal("unauthenticated limits read allowed")
	}
}

func TestBoundedLabelsCapCardinality(t *testing.T) {
	labels := newBoundedLabels(3)
	got := map[string]bool{}
	for _, value := range []string{"a", "b", "c", "d", "e", "a"} {
		got[labels.label(value)] = true
	}
	if len(got) != 4 || !got[overflowLabel] || !got["a"] || got["d"] {
		t.Fatalf("labels=%v", got)
	}
}
