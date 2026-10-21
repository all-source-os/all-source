package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func TestQueryMeterActualCorePersistsAcrossRestart(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	signer := func(user, tenant string, role entities.Role) (string, error) {
		return cp.authClient.SignAPIKey(user, tenant, role)
	}
	delegation, err := newDelegationClient(core.url, core.url, core.url, signer, &http.Client{Timeout: 2 * time.Second})
	if err != nil {
		t.Fatal(err)
	}
	cp.delegation = delegation
	cp.queryMeter = newQueryUsageMeter(cp.persistQueryUsage)
	t.Cleanup(func() {
		if err := cp.queryMeter.close(); err != nil {
			t.Error(err)
		}
	})
	for _, tenant := range []string{"meter-reader", "meter-other"} {
		workspaceJSON(t, cp.client, http.MethodPost, "/api/v1/tenants", map[string]any{
			"id": tenant, "name": "Synthetic metering test", "quota_preset": "unlimited",
			"metadata": map[string]any{"quotas": map[string]any{"queries_used": 0, "events_used": 7}},
		}, http.StatusCreated)
	}
	for range 3 {
		ctx, cancel := context.WithTimeout(t.Context(), 3*time.Second)
		request := httptest.NewRequestWithContext(ctx, http.MethodGet, "/api/v1/events/query?tenant_id=meter-other", http.NoBody)
		response := callHandler(cp.ProxyEventsQuery, request, "meter-reader")
		cancel()
		if response.Code != http.StatusOK || !strings.Contains(response.Body.String(), `"events"`) {
			t.Fatalf("actual Core query failed: %d %s", response.Code, response.Body.String())
		}
	}
	assertMeter := func(tenant string, want float64) {
		t.Helper()
		value := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/"+tenant, nil, http.StatusOK)
		quota := workspaceMap(t, workspaceMap(t, value["metadata"])["quotas"])
		if quota["queries_used"] != want || quota["events_used"] != float64(7) {
			t.Fatalf("wrong durable usage for %s: %#v", tenant, quota)
		}
	}
	cp.queryMeter.flush(t.Context())
	assertMeter("meter-reader", 3)
	assertMeter("meter-other", 0)
	core.restart(t)
	assertMeter("meter-reader", 3)
	assertMeter("meter-other", 0)
}
