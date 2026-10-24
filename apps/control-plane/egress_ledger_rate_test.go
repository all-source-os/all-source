//go:build egressintegration

package main

import (
	"os"
	"testing"
	"time"

	"github.com/google/uuid"
)

func TestEgressRealCoreBookkeepingDoesNotThrottleMetadata(t *testing.T) {
	if os.Getenv("ALLSOURCE_CORE_BINARY") == "" {
		t.Fatal("ALLSOURCE_CORE_BINARY must point to the tested multi-tenant Core binary")
	}
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	cp.client.SetTimeout(time.Second)
	workspaceJSON(t, cp.client, "POST", "/api/v1/tenants", map[string]any{
		"id": "ledger-load", "name": "ledger-load",
	}, 201)
	path := "/api/v1/tenants/ledger-load/egress"
	workspaceJSON(t, cp.client, "POST", path, map[string]any{
		"operation": "configure", "included_bytes": 1_000_000,
	}, 200)
	deadline := time.Now().Add(30 * time.Second)
	for range 900 {
		if time.Now().After(deadline) {
			t.Fatal("ledger load exceeded its bounded deadline")
		}
		id := uuid.NewString()
		workspaceJSON(t, cp.client, "POST", path, map[string]any{"operation": "reserve", "id": id}, 200)
		workspaceJSON(t, cp.client, "POST", path, map[string]any{"operation": egressOperationComplete, "id": id, "bytes": 0}, 200)
	}
	workspaceJSON(t, cp.client, "GET", "/api/v1/tenants/ledger-load", nil, 200)
	workspaceJSON(t, cp.client, "GET", path, nil, 200)
	t.Log("1,800 authenticated ledger operations preserved metadata reads and finite tenant admission")
}
