package main

import (
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"strconv"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/prometheus/client_golang/prometheus"
)

func meteringRouter(ledger *countingLedger) *gin.Engine {
	router := gin.New()
	router.Use(TenantMetering())
	router.Use(func(c *gin.Context) {
		if tenant := c.GetHeader("Test-Tenant"); tenant != "" {
			c.Set("auth_tenant_id", tenant)
		}
		c.Next()
	})
	cp := &ControlPlane{egressExchange: ledger.exchange, admission: newTenantAdmission(admissionShadow, defaultAdmissionLimits(), nil)}
	router.GET("/stream", func(c *gin.Context) {
		for range 5 {
			_, _ = c.Writer.WriteString(strings.Repeat("s", 333)) //nolint:errcheck // test stream
			c.Writer.Flush()
		}
	})
	router.GET("/cardinality-ping", func(c *gin.Context) { c.Status(http.StatusNoContent) })
	api := router.Group("/api/v1")
	api.Use(cp.enforceEgress)
	api.GET("/events/query", func(c *gin.Context) {
		size, _ := strconv.Atoi(c.Query("size")) //nolint:errcheck // test input
		c.String(http.StatusOK, strings.Repeat("x", size))
	})
	api.POST("/events", func(c *gin.Context) {
		body, _ := io.ReadAll(c.Request.Body) //nolint:errcheck // test input
		c.JSON(http.StatusCreated, gin.H{"read": len(body)})
	})
	return router
}

type meteredCall struct {
	tenant, method, target string
	body                   int
}

func TestTenantByteCountersReconcileWithClientBytes(t *testing.T) {
	ledger := &countingLedger{}
	server := httptest.NewServer(meteringRouter(ledger))
	defer server.Close()
	client := &http.Client{Transport: &http.Transport{DisableCompression: true}}
	calls := []meteredCall{
		{"recon-a", http.MethodGet, "/api/v1/events/query?size=10", 0},
		{"recon-a", http.MethodGet, "/api/v1/events/query?size=1000", 0},
		{"recon-a", http.MethodGet, "/api/v1/events/query?size=70000", 0},
		{"recon-b", http.MethodPost, "/api/v1/events", 100},
		{"recon-b", http.MethodPost, "/api/v1/events", 5000},
		{"recon-b", http.MethodGet, "/api/v1/events/query?size=5", 0},
		{"recon-c", http.MethodGet, "/stream", 0},
		{"recon-c", http.MethodGet, "/api/v1/events/query?size=1", 0},
		{"", http.MethodGet, "/api/v1/events/query?size=9", 0},
	}
	tenants := []string{"recon-a", "recon-b", "recon-c", anonymousTenant}
	responseBefore, requestBefore := map[string]float64{}, map[string]float64{}
	for _, tenant := range tenants {
		responseBefore[tenant] = counterValue(t, tenantResponseBytes, map[string]string{"tenant": tenant})
		requestBefore[tenant] = counterValue(t, tenantRequestBytes, map[string]string{"tenant": tenant})
	}
	received, sent := map[string]int{}, map[string]int{}
	for _, call := range calls {
		var body io.Reader = http.NoBody
		if call.body > 0 {
			body = strings.NewReader(`"` + strings.Repeat("b", call.body-2) + `"`)
		}
		request, err := http.NewRequestWithContext(t.Context(), call.method, server.URL+call.target, body)
		if err != nil {
			t.Fatal(err)
		}
		if call.tenant != "" {
			request.Header.Set("Test-Tenant", call.tenant)
		}
		response, err := client.Do(request)
		if err != nil {
			t.Fatal(err)
		}
		payload, err := io.ReadAll(response.Body)
		_ = response.Body.Close() //nolint:errcheck // test cleanup
		if err != nil {
			t.Fatal(err)
		}
		label := call.tenant
		if label == "" {
			label = anonymousTenant
		} else if response.StatusCode >= 300 {
			t.Fatalf("%s %s: %d %s", call.method, call.target, response.StatusCode, payload)
		}
		received[label] += len(payload)
		sent[label] += call.body
	}
	for _, tenant := range tenants {
		if got := counterValue(t, tenantResponseBytes, map[string]string{"tenant": tenant}) - responseBefore[tenant]; got != float64(received[tenant]) {
			t.Errorf("tenant %s response bytes counter=%v client received=%d", tenant, got, received[tenant])
		}
		if got := counterValue(t, tenantRequestBytes, map[string]string{"tenant": tenant}) - requestBefore[tenant]; got != float64(sent[tenant]) {
			t.Errorf("tenant %s request bytes counter=%v client sent=%d", tenant, got, sent[tenant])
		}
	}
	if received["recon-a"] != 71010 || received["recon-c"] != 1666 {
		t.Fatalf("fixture drifted: %v", received)
	}
	if got := counterValue(t, tenantRequests, map[string]string{"tenant": "recon-a", "route": "/api/v1/events/query", "method": http.MethodGet, "status_class": "2xx"}); got != 3 {
		t.Fatalf("recon-a 2xx query count=%v", got)
	}
	if got := counterValue(t, tenantRequests, map[string]string{"tenant": anonymousTenant, "route": "/api/v1/events/query", "status_class": "4xx"}); got < 1 {
		t.Fatal("unauthenticated refusal not metered as anonymous")
	}
}

func TestTenantLabelCardinalityIsCapped(t *testing.T) {
	restore := tenantLabels
	tenantLabels = newBoundedLabels(maxTenantLabels)
	t.Cleanup(func() { tenantLabels = restore })
	router := meteringRouter(&countingLedger{})
	for i := range maxTenantLabels + 44 {
		request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/cardinality-ping", http.NoBody)
		request.Header.Set("Test-Tenant", "card-"+strconv.Itoa(i))
		router.ServeHTTP(httptest.NewRecorder(), request)
	}
	registry := prometheus.NewRegistry()
	registry.MustRegister(tenantRequests)
	families, err := registry.Gather()
	if err != nil {
		t.Fatal(err)
	}
	distinct, overflow := map[string]bool{}, 0.0
	for _, family := range families {
		for _, metric := range family.GetMetric() {
			labels := map[string]string{}
			for _, label := range metric.GetLabel() {
				labels[label.GetName()] = label.GetValue()
			}
			if labels["route"] != "/cardinality-ping" {
				continue
			}
			distinct[labels["tenant"]] = true
			if labels["tenant"] == overflowLabel {
				overflow += metric.GetCounter().GetValue()
			}
		}
	}
	if len(distinct) != maxTenantLabels+1 || overflow != 44 {
		t.Fatalf("distinct tenant labels=%d overflow=%v", len(distinct), overflow)
	}
}

func scrape(t *testing.T, handler http.Handler) string {
	t.Helper()
	response := httptest.NewRecorder()
	handler.ServeHTTP(response, httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/metrics", http.NoBody))
	if response.Code != http.StatusOK {
		t.Fatalf("scrape: %d", response.Code)
	}
	return response.Body.String()
}

func TestPublicMetricsNeverCarryTenantLabels(t *testing.T) {
	router := meteringRouter(&countingLedger{})
	request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/events/query?size=3", http.NoBody)
	request.Header.Set("Test-Tenant", "scrape-tenant")
	router.ServeHTTP(httptest.NewRecorder(), request)
	admissionWouldRefuse.WithLabelValues(admissionLaneData, admissionScopeTenant, "scrape-tenant").Inc()

	for _, collector := range []prometheus.Collector{tenantResponseBytes, tenantRequestBytes, tenantRequests, tenantQueryBytes, tenantQueries, queryConditional, admissionWouldRefuse, admissionRefused} {
		err := prometheus.DefaultRegisterer.Register(collector)
		var already prometheus.AlreadyRegisteredError
		if errors.As(err, &already) {
			t.Fatalf("per-tenant collector is in the default registry: %v", err)
		}
		if err == nil {
			prometheus.DefaultRegisterer.Unregister(collector)
		}
	}
	public := scrape(t, publicMetricsHandler())
	for _, label := range []string{`tenant="`, `tenant_fp="`} {
		if strings.Contains(public, label) {
			t.Fatalf("public /metrics exposes %s", label)
		}
	}
	if !strings.Contains(public, "go_goroutines") {
		t.Fatal("public /metrics lost the default registry")
	}
	private := scrape(t, newPrivateMetricsServer(func(string) string { return "" }).Handler)
	for _, want := range []string{`control_plane_tenant_response_bytes_total{`, `tenant="scrape-tenant"`, `admission_would_refuse_total{`, "go_goroutines"} {
		if !strings.Contains(private, want) {
			t.Fatalf("private /metrics lacks %s", want)
		}
	}
}

func TestPrivateMetricsListenerIsNotThePublicPort(t *testing.T) {
	if addr := newPrivateMetricsServer(func(string) string { return "" }).Addr; addr != ":9091" {
		t.Fatalf("default METRICS_ADDR=%q", addr)
	}
	if addr := newPrivateMetricsServer(func(string) string { return "127.0.0.1:9999" }).Addr; addr != "127.0.0.1:9999" {
		t.Fatalf("METRICS_ADDR ignored: %q", addr)
	}
	fly, err := os.ReadFile("fly.toml")
	if err != nil {
		t.Fatal(err)
	}
	_, metrics, found := strings.Cut(string(fly), "[metrics]")
	if !found {
		t.Fatal("fly.toml has no [metrics] section")
	}
	metrics, _, _ = strings.Cut(metrics, "[[")
	if !strings.Contains(metrics, "port = 9091") || !strings.Contains(metrics, `path = "/metrics"`) {
		t.Fatalf("fly.toml [metrics] does not scrape the private listener: %s", metrics)
	}
	if strings.Contains(string(fly), "internal_port = 9091") {
		t.Fatal("private metrics port is exposed through [http_service]")
	}
}

func TestPrivateMetricsAnswerOnlyGet(t *testing.T) {
	handler := newPrivateMetricsServer(func(string) string { return "" }).Handler
	for method, want := range map[string]int{
		http.MethodGet:  http.StatusOK,
		http.MethodHead: http.StatusOK,
		http.MethodPost: http.StatusMethodNotAllowed,
		http.MethodPut:  http.StatusMethodNotAllowed,
	} {
		rec := httptest.NewRecorder()
		handler.ServeHTTP(rec, httptest.NewRequestWithContext(t.Context(), method, "/metrics", strings.NewReader("{}")))
		if rec.Code != want {
			t.Fatalf("%s /metrics = %d, want %d", method, rec.Code, want)
		}
		if method == http.MethodPost && strings.Contains(rec.Body.String(), "control_plane_tenant") {
			t.Fatal("a POST response carries tenant series")
		}
	}
}
