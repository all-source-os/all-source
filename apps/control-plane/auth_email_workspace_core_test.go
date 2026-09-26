package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func TestEmailWorkspaceRealCore(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	var authenticationFails atomic.Bool
	auth := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/auth/sign-up/email" && r.URL.Path != "/api/auth/sign-in/email" {
			t.Error("unexpected auth route")
			w.WriteHeader(404)
			return
		}
		if authenticationFails.Load() {
			w.WriteHeader(401)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		if err := json.NewEncoder(w).Encode(map[string]any{
			"token": "synthetic-auth-session",
			"user":  map[string]string{"id": "synthetic-email-owner", "email": "owner@example.test", "name": "Synthetic owner"},
		}); err != nil {
			t.Error("unable to encode auth fixture")
		}
	}))
	defer auth.Close()
	t.Setenv("AUTH_SERVICE_URL", auth.URL)
	t.Setenv("ADMIN_EMAILS", "owner@example.test")
	router := gin.New()
	router.POST("/register", cp.RegisterHandler)
	router.POST("/login", cp.LoginHandler)
	const tenant = "email-synthetic-email-owner"
	const subject = "synthetic-email-owner"

	authenticationFails.Store(true)
	emailWorkspaceRequest(t, router, "/register", 401)
	workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/"+tenant, nil, 404)
	authenticationFails.Store(false)
	registered := emailWorkspaceRequest(t, router, "/register", 201)
	token, ok := registered["token"].(string)
	if !ok || token == "synthetic-auth-session" {
		t.Fatal("auth service session leaked or human token missing")
	}
	claims, err := cp.authClient.ValidateToken(token)
	if err != nil || claims.TenantID != tenant || claims.UserID != subject || claims.IsAPIKey || claims.Role != entities.RoleDeveloper {
		t.Fatal("email signup changed authenticated identity or global privilege")
	}
	assertWorkspaceMember(t, cp, tenant, subject, roleAdmin)
	initial := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/"+tenant, nil, 200)
	metadata := workspaceMap(t, initial["metadata"])
	if workspaceMap(t, metadata["subscription"])["trial_expires_at"] == nil {
		t.Fatal("email signup did not persist trial expiry")
	}
	core.restart(t)
	returned := emailWorkspaceRequest(t, router, "/login", 200)
	if returned["new_user"] != false {
		t.Fatal("returning email identity was re-created")
	}
	current := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/"+tenant, nil, 200)
	if !bytes.Equal(workspaceBytes(t, metadata), workspaceBytes(t, current["metadata"])) {
		t.Fatal("returning email login reset trial")
	}
	paid := map[string]any{
		"subscription": map[string]any{"tier": "indie", "status": "active"},
		"quotas":       map[string]any{"mcp_scope": "read", "queries_quota": 50000, "queries_used": 123},
	}
	workspaceJSON(t, cp.client, http.MethodPut, "/api/v1/tenants/"+tenant, map[string]any{"metadata": paid}, 200)
	emailWorkspaceRequest(t, router, "/login", 200)
	current = workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/"+tenant, nil, 200)
	if !bytes.Equal(workspaceBytes(t, paid), workspaceBytes(t, current["metadata"])) {
		t.Fatal("returning email login reset paid subscription")
	}
	setWorkspaceConfig(t, cp, teamMembersConfigKey(tenant), []TeamMember{{UserID: subject, Role: roleMember}})
	emailWorkspaceRequest(t, router, "/login", 200)
	assertWorkspaceMember(t, cp, tenant, subject, roleMember)
	setWorkspaceConfig(t, cp, teamMembersConfigKey(tenant), []TeamMember{})
	emailWorkspaceRequest(t, router, "/login", 503)
	value, found, err := cp.workspaceConfig(t.Context(), teamMembersConfigKey(tenant))
	if err != nil || !found || string(value) != "[]" {
		t.Fatal("email login restored removed membership")
	}
}

func TestEmailWorkspaceRefusesUnstampedLegacyTenant(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	workspaceJSON(t, cp.client, http.MethodPost, "/api/v1/tenants", map[string]string{
		"id": "email-partial", "name": "Legacy partial account",
	}, 201)
	if _, _, err := cp.emailWorkspace(t.Context(), "partial", "owner@example.test", "Owner"); err == nil {
		t.Fatal("uninitialized legacy tenant was silently granted new ownership")
	}
	workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/config/team:email-partial:members", nil, 404)
	current := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/email-partial", nil, 200)
	if len(workspaceMap(t, current["metadata"])) != 0 {
		t.Fatal("legacy partial metadata was overwritten")
	}
}

func emailWorkspaceRequest(t *testing.T, router http.Handler, path string, status int) map[string]any {
	t.Helper()
	request := httptest.NewRequestWithContext(t.Context(), http.MethodPost, path, strings.NewReader(
		`{"name":"Synthetic owner","email":"owner@example.test","password":"SyntheticTestOnly123!"}`))
	request.Header.Set("Content-Type", "application/json")
	recorder := httptest.NewRecorder()
	router.ServeHTTP(recorder, request)
	if recorder.Code != status {
		t.Fatalf("email status %d, expected %d", recorder.Code, status)
	}
	var result map[string]any
	if json.Unmarshal(recorder.Body.Bytes(), &result) != nil {
		t.Fatal("invalid email response")
	}
	if status >= 400 && result["token"] != nil {
		t.Fatal("failed email setup returned a token")
	}
	return result
}
