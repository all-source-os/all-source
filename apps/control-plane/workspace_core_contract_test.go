package main

import (
	"net/http"
	"strings"
	"testing"
)

// These wire tests exercise the Core binary consumed by workspace provisioning,
// including capability detection and the existing unconditional config API.
func TestWorkspaceCoreConditionalHTTP(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	const path = "/api/v1/config/conditional/set"
	request := map[string]any{"key": "conditional", "value": "initial"}
	workspaceJSON(t, cp.client, http.MethodPost, path, request, 400)
	for _, condition := range []any{
		map[string]any{"kind": "unknown"},
		map[string]any{"kind": "revision", "revision": "not-a-uuid"},
		map[string]any{"kind": "absent", "unexpected": true},
	} {
		request["condition"] = condition
		workspaceJSON(t, cp.client, http.MethodPost, path, request, 422)
	}
	request["condition"] = map[string]string{"kind": "absent"}
	initial := workspaceJSON(t, cp.client, http.MethodPost, path, request, 200)
	workspaceJSON(t, cp.client, http.MethodPost, path, request, 409)
	// The distinct mutation route must not shadow the existing key "conditional".
	read := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/config/conditional", nil, 200)
	if initial["revision"] == nil || initial["revision"] != read["revision"] || read["value"] != "initial" {
		t.Fatal("conditional write acknowledgement did not match durable record")
	}
	core.restart(t)
	read = workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/config/conditional", nil, 200)
	if read["revision"] != initial["revision"] {
		t.Fatal("restart changed conditional revision")
	}
	request["condition"] = map[string]any{"kind": "revision", "revision": initial["revision"]}
	request["value"] = "consumed"
	updated := workspaceJSON(t, cp.client, http.MethodPost, path, request, 200)
	if updated["revision"] == initial["revision"] {
		t.Fatal("successful replacement reused a revision")
	}
	workspaceJSON(t, cp.client, http.MethodPost, path, request, 409)
	setWorkspaceConfig(t, cp, "conditional", "initial")
	workspaceJSON(t, cp.client, http.MethodPost, path, request, 409)
	workspaceJSON(t, cp.client, http.MethodDelete, "/api/v1/config/conditional", nil, 204)
	workspaceJSON(t, cp.client, http.MethodPost, path, request, 409)
	setWorkspaceConfig(t, cp, "conditional", "recreated")
	workspaceJSON(t, cp.client, http.MethodPost, path, request, 409)
	read = workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/config/conditional", nil, 200)
	if read["value"] != "recreated" {
		t.Fatal("stale revision modified recreated record")
	}
}

func TestWorkspaceCoreInitialMetadataHTTP(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	request := map[string]any{"id": "initial-metadata", "name": "Synthetic workspace", "quota_preset": "trial"}
	for _, metadata := range []any{"invalid", []string{"invalid"}, map[string]any{"oversized": strings.Repeat("x", 32_768)}} {
		request["metadata"] = metadata
		workspaceJSON(t, cp.client, http.MethodPost, "/api/v1/tenants", request, 400)
		workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/initial-metadata", nil, 404)
	}
	request["metadata"] = map[string]any{"subscription": map[string]any{"tier": "trial"}, "marker": "initial"}
	request["description"] = "Synthetic initialization proof"
	request["is_demo"] = true
	created := workspaceJSON(t, cp.client, http.MethodPost, "/api/v1/tenants", request, 201)
	core.restart(t)
	recovered := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/initial-metadata", nil, 200)
	for _, tenant := range []map[string]any{created, recovered} {
		if tenant["description"] != request["description"] || tenant["is_demo"] != true || workspaceMap(t, tenant["metadata"])["marker"] != "initial" {
			t.Fatal("tenant initialization fields did not persist together")
		}
	}
	request["metadata"] = map[string]string{"marker": "replacement"}
	workspaceJSON(t, cp.client, http.MethodPost, "/api/v1/tenants", request, 409)
	current := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/initial-metadata", nil, 200)
	if workspaceMap(t, current["metadata"])["marker"] != "initial" {
		t.Fatal("duplicate creation overwrote metadata")
	}
}

func TestWorkspaceCoreFollowerRejectsProvisioning(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	setWorkspaceConfig(t, cp, "follower-proof", "leader-value")
	if _, err := cp.findOrCreateOAuthUser("google", "follower-owner", "follower@example.test", "Owner", ""); err != nil {
		t.Fatal("leader could not establish synthetic owner")
	}
	core.stop(t)
	core.role = "follower"
	core.start(t)
	if result, err := cp.findOrCreateOAuthUser("google", "follower-owner", "follower@example.test", "Owner", ""); err == nil || result != nil {
		t.Fatal("follower state authorized returning owner session")
	}
	workspaceJSON(t, cp.client, http.MethodPost, "/api/v1/config/conditional/set", map[string]any{
		"key": "follower-proof", "value": "forbidden", "condition": map[string]string{"kind": "absent"},
	}, 409)
	workspaceJSON(t, cp.client, http.MethodPost, "/api/v1/tenants", map[string]any{
		"id": "follower-tenant", "name": "Forbidden", "metadata": map[string]string{"marker": "forbidden"},
	}, 409)
	workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/follower-tenant", nil, 404)
	read := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/config/follower-proof", nil, 200)
	if read["value"] != "leader-value" {
		t.Fatal("follower modified configuration")
	}
}
