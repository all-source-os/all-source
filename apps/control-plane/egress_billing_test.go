package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
)

func TestEgressUnitMatchesTheBytesTheGuardCharges(t *testing.T) {
	if egressChargeUnit != "response_body_bytes_as_sent_after_compression" {
		t.Fatalf("advertised unit changed: %q", egressChargeUnit)
	}
	compressed := "\x1f\x8b\x08\x00compressed-body"
	ledger := &testEgressLedger{left: map[string]uint64{"unit-tenant": 1 << 20}, reservations: map[string]uint64{}}
	cp := &ControlPlane{egressExchange: ledger.exchange}
	router := gin.New()
	router.Use(func(c *gin.Context) { c.Set("auth_tenant_id", "unit-tenant"); c.Next() })
	api := router.Group("/api/v1")
	api.Use(cp.enforceEgress)
	api.GET("/views/:view/entities", func(c *gin.Context) {
		c.Header("Content-Encoding", "gzip")
		c.Data(http.StatusOK, "application/json", []byte(compressed))
	})
	request := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/views/ops/entities", http.NoBody)
	request.Header.Set("Accept-Encoding", "gzip")
	response := httptest.NewRecorder()
	router.ServeHTTP(response, request)
	if response.Code != http.StatusOK || response.Body.String() != compressed {
		t.Fatalf("status=%d body=%q", response.Code, response.Body.String())
	}
	if len(ledger.settlements) != 1 || ledger.settlements[0].Bytes != uint64(len(compressed)) {
		t.Fatalf("settled %v, want the %d encoded bytes as sent", ledger.settlements, len(compressed))
	}
}

func TestEgressUsageAdvertisesTheChargedUnit(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/tenants/unit-tenant/egress" {
			t.Errorf("usage read %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"included_used":1}`)) //nolint:errcheck // test response
	}))
	defer server.Close()
	cp := newTestCP(t, server.URL, server.URL)
	response := callHandler(cp.egressUsage, httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/billing/egress", http.NoBody), "unit-tenant")
	var body struct {
		Unit string `json:"unit"`
	}
	if response.Code != http.StatusOK || json.Unmarshal(response.Body.Bytes(), &body) != nil || body.Unit != egressChargeUnit {
		t.Fatalf("status=%d body=%s", response.Code, response.Body.String())
	}
}
