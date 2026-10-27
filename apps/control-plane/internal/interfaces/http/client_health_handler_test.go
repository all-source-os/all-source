package http

import (
	"bytes"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

var healthNow = time.Date(2026, 10, 26, 12, 0, 0, 0, time.UTC)

type promVec map[string]float64 // "k=v,k=v" -> value

type promScenario struct {
	nowMix     promVec
	series     map[string][][2]float64 // status -> [unix, value]
	tenants    promVec                 // tenant mix over [5m]
	tenants15  promVec                 // same mix over [15m]; served only if the window regresses
	baseline   promVec
	starts     map[string][][2]float64 // "app=..,instance=.." -> [unix, start]
	up         promVec
	uptime     promVec
	failStatus int
	partial    bool
	delay      time.Duration
}

func parseLabels(s string) map[string]string {
	out := map[string]string{}
	for _, kv := range strings.Split(s, ",") {
		if k, v, ok := strings.Cut(kv, "="); ok {
			out[k] = v
		}
	}
	return out
}

func vectorJSON(v promVec, at time.Time) map[string]any {
	result := make([]map[string]any, 0, len(v))
	for labels, val := range v {
		result = append(result, map[string]any{
			"metric": parseLabels(labels),
			"value":  []any{float64(at.Unix()), fmt.Sprintf("%g", val)},
		})
	}
	return map[string]any{"status": "success", "data": map[string]any{"resultType": "vector", "result": result}}
}

func matrixJSON(m map[string][][2]float64, labelKey string) map[string]any {
	result := make([]map[string]any, 0, len(m))
	for labels, pts := range m {
		values := make([][]any, 0, len(pts))
		for _, p := range pts {
			values = append(values, []any{p[0], fmt.Sprintf("%f", p[1])})
		}
		metric := parseLabels(labels)
		if labelKey != "" {
			metric = map[string]string{labelKey: labels}
		}
		result = append(result, map[string]any{"metric": metric, "values": values})
	}
	return map[string]any{"status": "success", "data": map[string]any{"resultType": "matrix", "result": result}}
}

type fakeProm struct {
	srv       *httptest.Server
	hits      atomic.Int64
	gotAuth   atomic.Value
	rangeStep atomic.Value
}

func newFakeProm(t *testing.T, sc *promScenario) *fakeProm {
	t.Helper()
	f := &fakeProm{}
	f.srv = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		f.hits.Add(1)
		f.gotAuth.Store(r.Header.Get("Authorization"))
		if sc.delay > 0 {
			time.Sleep(sc.delay)
		}
		w.Header().Set("Content-Type", "application/json")
		if sc.failStatus != 0 {
			w.WriteHeader(sc.failStatus)
			_, _ = fmt.Fprintf(w, `{"status":"error","errorType":"internal","error":"boom; auth was %s"}`, r.Header.Get("Authorization")) //nolint:errcheck // test response
			return
		}
		q := r.URL.Query().Get("query")
		var body map[string]any
		switch {
		case r.URL.Path == "/api/v1/query_range" && strings.Contains(q, "fly_instance_uptime_seconds"):
			body = matrixJSON(sc.starts, "")
		case r.URL.Path == "/api/v1/query_range":
			f.rangeStep.Store(r.URL.Query().Get("step"))
			body = matrixJSON(sc.series, "status")
		case strings.Contains(q, "[24h]"):
			body = vectorJSON(sc.baseline, healthNow)
		case strings.Contains(q, "status_class") && strings.Contains(q, "[15m]"):
			body = vectorJSON(sc.tenants15, healthNow)
		case strings.Contains(q, "status_class"):
			body = vectorJSON(sc.tenants, healthNow)
		case strings.Contains(q, "fly_instance_up{"):
			body = vectorJSON(sc.up, healthNow)
		case strings.Contains(q, "fly_instance_uptime_seconds"):
			body = vectorJSON(sc.uptime, healthNow)
		default:
			body = vectorJSON(sc.nowMix, healthNow)
		}
		if sc.partial {
			body["isPartial"] = true
		}
		_ = json.NewEncoder(w).Encode(body) //nolint:errcheck // test response
	}))
	t.Cleanup(f.srv.Close)
	return f
}

func clientHealthRouter(uc *usecases.ClientHealthUseCase) *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.New()
	admin := r.Group("/api/v1/admin")
	admin.Use(AdminAuthMiddleware(fleetTestJWTSecret))
	admin.Group("/metrics").GET("/client-health", NewClientHealthHandler(uc).Get)
	return r
}

func unix(d time.Duration) float64 { return float64(healthNow.Add(d).Unix()) }

func normalScenario() *promScenario {
	return &promScenario{
		nowMix: promVec{"status=200": 5800.4, "status=429": 12, "status=504": 1},
		series: map[string][][2]float64{
			"200": {{unix(-2 * time.Minute), 1180}, {unix(-time.Minute), 1175}},
			"401": {{unix(-time.Minute), 2}},
		},
		tenants:   promVec{"tenant=onboard-control-plane-at-technical-leaders-com,status_class=2xx": 5800},
		tenants15: promVec{"tenant=onboard-control-plane-at-technical-leaders-com,status_class=2xx": 17500},
		baseline:  promVec{"tenant=onboard-control-plane-at-technical-leaders-com": 1200, "tenant=anonymous": 13.9},
		starts: map[string][][2]float64{
			"app=allsource-control-plane,instance=849100f2140768": {
				{unix(-50 * time.Minute), unix(-72 * time.Hour)},
				{unix(-30 * time.Minute), unix(-31*time.Minute) + 0.3},
				{unix(-29 * time.Minute), unix(-31*time.Minute) + 0.31},
			},
		},
		up:     promVec{"app=allsource-control-plane,instance=849100f2140768": 1, "app=allsource-core,instance=85777d6c1d17d8": 1},
		uptime: promVec{"app=allsource-control-plane,instance=849100f2140768": 1860, "app=allsource-core,instance=85777d6c1d17d8": 8985},
	}
}

// outageScenario is six minutes into an auth outage, shaped on 22:50Z
// 2026-10-26: the 5m window holds only refused traffic while the 15m window
// still holds the traffic served before the outage began.
func outageScenario() *promScenario {
	sc := normalScenario()
	sc.nowMix = promVec{"status=401": 821, "status=200": 3}
	sc.tenants = promVec{"tenant=anonymous,status_class=4xx": 626}
	sc.tenants15 = promVec{
		"tenant=anonymous,status_class=4xx":                                      900,
		"tenant=onboard-control-plane-at-technical-leaders-com,status_class=2xx": 10520,
	}
	sc.baseline = promVec{"tenant=onboard-control-plane-at-technical-leaders-com": 1187.4}
	return sc
}

// longOutageScenario is two hours into an auth outage, shaped on 21:00Z
// 2026-10-26: the tenant has served nothing for longer than an hour.
func longOutageScenario() *promScenario {
	sc := normalScenario()
	sc.nowMix = promVec{"status=401": 359}
	sc.tenants = promVec{"tenant=anonymous,status_class=4xx": 359}
	sc.tenants15 = sc.tenants
	sc.baseline = promVec{"tenant=onboard-control-plane-at-technical-leaders-com": 237.8, "tenant=anonymous": 6.2}
	return sc
}

func tenantByName(rep usecases.ClientHealthReport) map[string]usecases.TenantHealth {
	out := map[string]usecases.TenantHealth{}
	for _, th := range rep.Tenants {
		out[th.Tenant] = th
	}
	return out
}

func getHealth(t *testing.T, r *gin.Engine, rng string) (code int, rep usecases.ClientHealthReport, raw string) {
	t.Helper()
	w := doAdmin(t, r, http.MethodGet, "/api/v1/admin/metrics/client-health?range="+rng, adminToken(t, entities.RoleAdmin), nil)
	if err := json.Unmarshal(w.Body.Bytes(), &rep); err != nil {
		t.Fatalf("unmarshal %q: %v", w.Body.String(), err)
	}
	return w.Code, rep, w.Body.String()
}

func TestClientHealth(t *testing.T) {
	tests := []struct {
		name      string
		scenario  *promScenario
		token     string
		rng       string
		wantCode  int
		check     func(t *testing.T, rep usecases.ClientHealthReport, raw string)
		wantAuthz string
	}{
		{
			name:     "not configured",
			rng:      "1h",
			wantCode: http.StatusOK,
			check: func(t *testing.T, rep usecases.ClientHealthReport, raw string) {
				if rep.Configured || rep.Now != nil || strings.Contains(raw, `"now"`) || strings.Contains(raw, `"series"`) {
					t.Errorf("unconfigured report carries numbers: %s", raw)
				}
			},
		},
		{
			name:      "normal",
			scenario:  normalScenario(),
			token:     "plain-token",
			rng:       "1h",
			wantCode:  http.StatusOK,
			wantAuthz: "Bearer plain-token",
			check: func(t *testing.T, rep usecases.ClientHealthReport, _ string) {
				if !rep.Configured || rep.StepSeconds != 60 || rep.Range != "1h" {
					t.Fatalf("header fields: %+v", rep)
				}
				n := rep.Now
				if n == nil || n.Served != 5800 || n.Throttled != 12 || n.Failed != 1 || n.Total != 5813 || n.WindowSeconds != 300 {
					t.Fatalf("now = %+v", n)
				}
				if n.ServedRatio == nil || *n.ServedRatio < 0.99 {
					t.Errorf("served_ratio = %v", n.ServedRatio)
				}
				if len(rep.Series) != 2 || rep.Series[1].Served != 1175 || rep.Series[1].RefusedAuth != 2 {
					t.Errorf("series = %+v", rep.Series)
				}
				if len(rep.Tenants) != 2 || rep.Tenants[0].State != usecases.TenantStateServed || rep.Tenants[0].Served5m != 5800 ||
					rep.Tenants[1].Tenant != "anonymous" || rep.Tenants[1].State != usecases.TenantStateIdle {
					t.Errorf("tenants = %+v", rep.Tenants)
				}
				if len(rep.Releases) != 1 || rep.Releases[0].StartedAt != healthNow.Add(-31*time.Minute).Format(time.RFC3339) {
					t.Errorf("releases = %+v", rep.Releases)
				}
				if len(rep.Machines) != 2 || !rep.Machines[0].Up || rep.Machines[0].UptimeSeconds != 1860 ||
					rep.Machines[0].StartedAt != healthNow.Add(-1860*time.Second).Format(time.RFC3339) {
					t.Errorf("machines = %+v", rep.Machines)
				}
			},
		},
		{
			name:      "outage: 401 dominates and the tenant goes quiet",
			scenario:  outageScenario(),
			token:     "FlyV1 fm2_abc,fm2_def",
			rng:       "24h",
			wantCode:  http.StatusOK,
			wantAuthz: "FlyV1 fm2_abc,fm2_def",
			check: func(t *testing.T, rep usecases.ClientHealthReport, _ string) {
				n := rep.Now
				if n == nil || n.RefusedAuth != 821 || n.ServedRatio == nil || *n.ServedRatio > 0.01 {
					t.Fatalf("now = %+v", n)
				}
				if rep.StepSeconds != 300 {
					t.Errorf("step = %d, want 300", rep.StepSeconds)
				}
				byTenant := tenantByName(rep)
				if got := byTenant["onboard-control-plane-at-technical-leaders-com"]; got.State != usecases.TenantStateQuiet || got.Served5m != 0 {
					t.Errorf("tenant = %+v, want quiet with 0 served", got)
				}
				if got := byTenant["anonymous"]; got.State != usecases.TenantStateRefused || got.Refused5m != 626 {
					t.Errorf("anonymous = %+v, want refused 626", got)
				}
				if rep.Tenants[0].State != usecases.TenantStateQuiet {
					t.Errorf("quiet tenant not first: %+v", rep.Tenants)
				}
			},
		},
		{
			name:      "long outage: the tenant stays listed as quiet",
			scenario:  longOutageScenario(),
			token:     "fm2_rawmacaroon",
			rng:       "24h",
			wantCode:  http.StatusOK,
			wantAuthz: "FlyV1 fm2_rawmacaroon",
			check: func(t *testing.T, rep usecases.ClientHealthReport, _ string) {
				got, ok := tenantByName(rep)["onboard-control-plane-at-technical-leaders-com"]
				if !ok || got.State != usecases.TenantStateQuiet || got.BaselinePerMin != 237.8 {
					t.Errorf("tenant = %+v (listed %v), want quiet with baseline 237.8", got, ok)
				}
			},
		},
		{
			name:     "7d range uses an hourly step",
			scenario: normalScenario(),
			token:    "tok",
			rng:      "7d",
			wantCode: http.StatusOK,
			check: func(t *testing.T, rep usecases.ClientHealthReport, _ string) {
				if rep.StepSeconds != 3600 {
					t.Errorf("step = %d, want 3600", rep.StepSeconds)
				}
			},
		},
		{
			name: "anonymous with no traffic is not listed",
			scenario: func() *promScenario {
				sc := normalScenario()
				sc.baseline = promVec{"tenant=onboard-control-plane-at-technical-leaders-com": 1200}
				return sc
			}(),
			token:    "tok",
			rng:      "1h",
			wantCode: http.StatusOK,
			check: func(t *testing.T, rep usecases.ClientHealthReport, _ string) {
				if _, ok := tenantByName(rep)["anonymous"]; ok || len(rep.Tenants) != 1 {
					t.Errorf("tenants = %+v, want only the served tenant", rep.Tenants)
				}
			},
		},
		{
			name:     "partial upstream result is a 502",
			scenario: func() *promScenario { sc := normalScenario(); sc.partial = true; return sc }(),
			token:    "tok",
			rng:      "1h",
			wantCode: http.StatusBadGateway,
			check: func(t *testing.T, rep usecases.ClientHealthReport, raw string) {
				if !strings.Contains(rep.Error, "partial result") || rep.Now != nil {
					t.Errorf("partial: %s", raw)
				}
			},
		},
		{
			name:     "upstream 500 is a 502 with no numbers",
			scenario: &promScenario{failStatus: http.StatusInternalServerError},
			token:    "FlyV1 fm2_secret-token-value",
			rng:      "1h",
			wantCode: http.StatusBadGateway,
			check: func(t *testing.T, rep usecases.ClientHealthReport, raw string) {
				if rep.Error == "" || !strings.Contains(rep.Error, "HTTP 500") {
					t.Errorf("error = %q", rep.Error)
				}
				if rep.Now != nil || strings.Contains(raw, `"now"`) {
					t.Errorf("502 carries numbers: %s", raw)
				}
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var logs bytes.Buffer
			log.SetOutput(&logs)
			t.Cleanup(func() { log.SetOutput(os.Stderr) })

			var querier usecases.PromQuerier
			var fake *fakeProm
			if tt.scenario != nil {
				fake = newFakeProm(t, tt.scenario)
				querier = clients.NewPrometheusClient(fake.srv.Client(), fake.srv.URL, tt.token)
			}
			uc := usecases.NewClientHealthUseCase(querier).WithClock(func() time.Time { return healthNow })
			code, rep, raw := getHealth(t, clientHealthRouter(uc), tt.rng)
			if code != tt.wantCode {
				t.Fatalf("code = %d, want %d (%s)", code, tt.wantCode, raw)
			}
			tt.check(t, rep, raw)
			if tt.rng == "7d" {
				if got, ok := fake.rangeStep.Load().(string); !ok || got != "3600" {
					t.Errorf("query_range step = %q, want 3600", got)
				}
			}
			if tt.wantAuthz != "" {
				if got, ok := fake.gotAuth.Load().(string); !ok || got != tt.wantAuthz {
					t.Errorf("Authorization = %q, want %q", got, tt.wantAuthz)
				}
			}
			if tt.token != "" {
				secret := strings.TrimPrefix(tt.token, "FlyV1 ")
				if strings.Contains(raw, secret) || strings.Contains(logs.String(), secret) {
					t.Errorf("token leaked; body=%s logs=%s", raw, logs.String())
				}
			}
		})
	}
}

func TestClientHealth_CacheHit(t *testing.T) {
	fake := newFakeProm(t, normalScenario())
	clock := healthNow
	uc := usecases.NewClientHealthUseCase(clients.NewPrometheusClient(fake.srv.Client(), fake.srv.URL, "tok")).
		WithClock(func() time.Time { return clock })
	r := clientHealthRouter(uc)

	getHealth(t, r, "1h")
	first := fake.hits.Load()
	if first == 0 {
		t.Fatal("no upstream queries on first call")
	}
	getHealth(t, r, "1h")
	if got := fake.hits.Load(); got != first {
		t.Errorf("cached call queried upstream: %d -> %d", first, got)
	}
	clock = clock.Add(16 * time.Second)
	getHealth(t, r, "1h")
	if got := fake.hits.Load(); got != 2*first {
		t.Errorf("expired cache: hits = %d, want %d", got, 2*first)
	}
}

func TestClientHealth_ConcurrentMissesShareOneFetch(t *testing.T) {
	sc := normalScenario()
	sc.delay = 50 * time.Millisecond
	fake := newFakeProm(t, sc)
	uc := usecases.NewClientHealthUseCase(clients.NewPrometheusClient(fake.srv.Client(), fake.srv.URL, "tok")).
		WithClock(func() time.Time { return healthNow })
	r := clientHealthRouter(uc)

	getHealth(t, r, "24h")
	perFetch := fake.hits.Load()
	fake.hits.Store(0)

	var wg sync.WaitGroup
	for range 4 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			w := doAdmin(t, r, http.MethodGet, "/api/v1/admin/metrics/client-health?range=1h", adminToken(t, entities.RoleAdmin), nil)
			if w.Code != http.StatusOK {
				t.Errorf("code = %d", w.Code)
			}
		}()
	}
	wg.Wait()
	if got := fake.hits.Load(); got != perFetch {
		t.Errorf("4 concurrent misses made %d upstream queries, want %d", got, perFetch)
	}
}

func TestTenantState(t *testing.T) {
	tests := []struct {
		name                    string
		served, refused, failed int64
		baseline                float64
		want                    string
	}{
		{"baseline just below the quiet floor", 0, 0, 0, 9.9, usecases.TenantStateIdle},
		{"baseline at the quiet floor", 0, 0, 0, 10.0, usecases.TenantStateQuiet},
		{"served under 5% of the baseline", 49, 0, 0, 200, usecases.TenantStateQuiet},
		{"served at 5% of the baseline", 50, 0, 0, 200, usecases.TenantStateServed},
		{"refused equal to served is not refused", 100, 100, 0, 20, usecases.TenantStateServed},
		{"refused one more than served", 100, 101, 0, 20, usecases.TenantStateRefused},
		{"named tenant with only 4xx", 0, 40, 0, 20, usecases.TenantStateRefused},
		{"small tenant served", 3, 0, 0, 0.5, usecases.TenantStateServed},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := usecases.TenantState(tt.served, tt.refused, tt.failed, tt.baseline); got != tt.want {
				t.Errorf("TenantState = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestPrometheusAuthHeader(t *testing.T) {
	tests := []struct{ token, want string }{
		{"FlyV1 fm2_abc,fm2_def", "FlyV1 fm2_abc,fm2_def"},
		{"fm2_abc,fm2_def", "FlyV1 fm2_abc,fm2_def"},
		{"fm1r_abc", "FlyV1 fm1r_abc"},
		{"fo1_abc", "FlyV1 fo1_abc"},
		{"fm2_abc\n", "FlyV1 fm2_abc"},
		{"plain-token", "Bearer plain-token"},
	}
	for _, tt := range tests {
		if got := clients.PrometheusAuthHeader(tt.token); got != tt.want {
			t.Errorf("PrometheusAuthHeader(%q) = %q, want %q", tt.token, got, tt.want)
		}
	}
}

func TestClientHealth_InvalidRange(t *testing.T) {
	uc := usecases.NewClientHealthUseCase(nil)
	w := doAdmin(t, clientHealthRouter(uc), http.MethodGet, "/api/v1/admin/metrics/client-health?range=30d", adminToken(t, entities.RoleAdmin), nil)
	if w.Code != http.StatusBadRequest {
		t.Errorf("code = %d, want 400", w.Code)
	}
}

func TestClientHealth_RequiresAdmin(t *testing.T) {
	r := clientHealthRouter(usecases.NewClientHealthUseCase(nil))
	if w := doAdmin(t, r, http.MethodGet, "/api/v1/admin/metrics/client-health", "", nil); w.Code != http.StatusUnauthorized {
		t.Errorf("no token: %d", w.Code)
	}
	if w := doAdmin(t, r, http.MethodGet, "/api/v1/admin/metrics/client-health", adminToken(t, entities.RoleDeveloper), nil); w.Code != http.StatusForbidden {
		t.Errorf("developer: %d", w.Code)
	}
}
