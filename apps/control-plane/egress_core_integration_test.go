//go:build egressintegration

package main

import (
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func TestEgressRealCoreHardStopAndRestart(t *testing.T) {
	if os.Getenv("ALLSOURCE_CORE_BINARY") == "" {
		t.Fatal("ALLSOURCE_CORE_BINARY must point to the tested multi-tenant Core binary")
	}
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	var err error
	cp.delegation, err = newDelegationClient(core.url, core.url, core.url, cp.authClient.SignAPIKey, nil)
	if err != nil {
		t.Fatal(err)
	}
	for _, tenant := range []string{"abusive", "healthy"} {
		workspaceJSON(t, cp.client, "POST", "/api/v1/tenants", map[string]any{
			"id": tenant, "name": tenant, "metadata": map[string]any{"subscription": map[string]any{"tier": "indie", "status": "active"}},
		}, 201)
		workspaceJSON(t, cp.client, "POST", "/api/v1/tenants/"+tenant+"/egress", map[string]any{"operation": "configure", "included_bytes": 6}, 200)
	}
	calls := 0
	router := egressTestRouter(cp.exchangeEgress, func(c *gin.Context) {
		calls++
		c.String(200, "abc")
	})
	request := func(tenant string, status int) {
		t.Helper()
		req := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/events/query?tenant_id=healthy", http.NoBody)
		req.Header.Set("Test-Tenant", tenant)
		req.Header.Set("Authorization", "rotated-client-key")
		response := httptest.NewRecorder()
		router.ServeHTTP(response, req)
		if response.Code != status {
			t.Fatalf("tenant %s: got %d want %d: %s", tenant, response.Code, status, response.Body.String())
		}
		if status == 200 && response.Body.String() != "abc" {
			t.Fatal("healthy response changed")
		}
	}
	request("abusive", 200)
	request("abusive", 200)
	for range 20 {
		request("abusive", 402)
	}
	if calls != 2 {
		t.Fatalf("exhausted client reached upstream %d times", calls)
	}
	request("healthy", 200)
	core.restart(t)
	request("abusive", 402)
	for range 2 {
		workspaceJSON(t, cp.client, "POST", "/api/v1/tenants/abusive/egress", map[string]any{
			"operation": "payment", "payment_id": "verified-order-1", "bytes": 3, "refunded": false,
		}, 200)
	}
	request("abusive", 200)
	request("abusive", 402)
	workspaceJSON(t, cp.client, "POST", "/api/v1/tenants/abusive/egress", map[string]any{
		"operation": "payment", "payment_id": "refund-first", "bytes": 3, "refunded": true,
	}, 200)
	workspaceJSON(t, cp.client, "POST", "/api/v1/tenants/abusive/egress", map[string]any{
		"operation": "payment", "payment_id": "refund-first", "bytes": 3, "refunded": false,
	}, 200)
	core.restart(t)
	request("abusive", 402)
	request("healthy", 200)
	workspaceJSON(t, cp.client, "POST", "/api/v1/tenants/healthy/egress", map[string]any{"operation": "configure", "included_bytes": 65536}, 200)
	workspaceJSON(t, cp.client, "POST", "/api/v1/events", map[string]any{
		"tenant_id": "healthy", "entity_id": "workflow-1", "event_type": "workflow.drafted", "payload": map[string]any{"state": "review"},
	}, 200)
	cp.queryMeter = newQueryUsageMeter(cp.persistQueryUsage)
	t.Cleanup(func() {
		if err := cp.queryMeter.close(); err != nil {
			t.Error(err)
		}
	})
	cp.egressExchange = cp.exchangeEgress
	funded := gin.New()
	funded.Use(AuthMiddleware(cp.authClient), cp.enforceEgress)
	funded.GET("/api/v1/events/query", cp.ProxyEventsQuery)
	for _, username := range []string{"client-key-1", "client-key-2"} {
		token, err := cp.authClient.IssueManagedAPIKey(t.Context(), "abusive", username, entities.RoleDeveloper)
		if err != nil {
			t.Fatal(err)
		}
		req := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/events/query?tenant_id=healthy&limit=1", http.NoBody)
		req.Header.Set("Authorization", "Bearer "+token)
		response := httptest.NewRecorder()
		funded.ServeHTTP(response, req)
		if response.Code != 402 {
			t.Fatalf("rotated credential bypassed tenant budget: %d", response.Code)
		}
	}
	for range 2 {
		token, err := cp.authClient.IssueManagedAPIKey(t.Context(), "healthy", "rotated-reader", entities.RoleDeveloper)
		if err != nil {
			t.Fatal(err)
		}
		req := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/api/v1/events/query?entity_id=workflow-1&limit=1&tenant_id=abusive", http.NoBody)
		req.Header.Set("Authorization", "Bearer "+token)
		response := httptest.NewRecorder()
		funded.ServeHTTP(response, req)
		if response.Code != 200 || !strings.Contains(response.Body.String(), "workflow.drafted") || !strings.Contains(response.Body.String(), "review") {
			t.Fatalf("funded workflow query failed: %d %s", response.Code, response.Body.String())
		}
	}
	if err := cp.queryMeter.close(); err != nil {
		t.Fatal(err)
	}
	t.Log("Core HTTP ledger: 6-byte hard stop, 20 refused upstream calls, rotated signed credentials, normal workflow reads, duplicate credit, refund replay and two durable restarts verified")
}
