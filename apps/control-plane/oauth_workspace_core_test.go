package main

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"os"
	"strings"
	"sync"
	"sync/atomic"
	"testing"

	"github.com/go-resty/resty/v2"

	"github.com/allsource/control-plane/internal/domain/entities"
)

// This test uses an actual Core process. Provider authentication is outside its
// scope; the exercised entry point is the real post-provider login function.
func TestOAuthWorkspaceRealCore(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	ctx := t.Context()
	t.Setenv("ADMIN_EMAILS", "")
	const subject = "oauth:google:synthetic-owner"
	result, err := cp.findOrCreateOAuthUser("google", "synthetic-owner", "owner@example.test", "Synthetic owner", "")
	if err != nil || result == nil || !result.IsNewUser {
		t.Fatalf("new owner setup failed: %v", err)
	}
	claims, err := cp.authClient.ValidateToken(result.Token)
	if err != nil || claims.UserID != subject || claims.TenantID != result.TenantID || claims.Role != entities.RoleDeveloper || claims.IsAPIKey {
		t.Fatal("incorrect human session scope")
	}
	tenantID := result.TenantID
	assertWorkspaceMember(t, cp, tenantID, subject, roleAdmin)
	tenant := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/"+tenantID, nil, 200)
	metadata := workspaceMap(t, tenant["metadata"])
	subscription := workspaceMap(t, metadata["subscription"])
	if subscription["tier"] != "trial" || subscription["trial_expires_at"] == nil {
		t.Fatal("trial expiry was not durably initialized")
	}

	// New CP instances, concurrent callbacks and Core restart share the same
	// owner record; no process-local registry or repeated team overwrite.
	var wg sync.WaitGroup
	for range 6 {
		wg.Go(func() {
			other := core.controlPlane(t, core.url)
			next, err := other.findOrCreateOAuthUser("google", "synthetic-owner", "changed@example.test", "Changed name", "")
			if err != nil || next == nil || next.TenantID != tenantID || next.IsNewUser {
				t.Errorf("concurrent returning login changed workspace: %v", err)
			}
		})
	}
	wg.Wait()
	core.restart(t)
	reopened := core.controlPlane(t, core.url)
	next, err := reopened.findOrCreateOAuthUser("google", "synthetic-owner", "changed@example.test", "Changed name", "")
	if err != nil || next == nil || next.TenantID != tenantID {
		t.Fatalf("restart lost owner registry: %v", err)
	}
	assertWorkspaceMember(t, reopened, tenantID, subject, roleAdmin)

	// Current role changes survive login. Empty membership remains revoked.
	memberKey := teamMembersConfigKey(tenantID)
	setWorkspaceConfig(t, reopened, memberKey, []TeamMember{{UserID: subject, Role: roleMember}})
	if _, err := reopened.findOrCreateOAuthUser("google", "synthetic-owner", "owner@example.test", "Owner", ""); err != nil {
		t.Fatal(err)
	}
	assertWorkspaceMember(t, reopened, tenantID, subject, roleMember)
	setWorkspaceConfig(t, reopened, memberKey, []TeamMember{})
	if denied, err := reopened.findOrCreateOAuthUser("google", "synthetic-owner", "owner@example.test", "Owner", ""); err == nil || denied != nil {
		t.Fatal("removed member was restored or issued a new session")
	}
	setWorkspaceConfig(t, reopened, memberKey, []TeamMember{{UserID: subject, Role: roleAdmin}})

	// A paid subscription and used quota are never reset by a returning login.
	paid := map[string]any{
		"subscription": map[string]any{"tier": "indie", "status": "active"},
		"quotas":       map[string]any{"mcp_scope": "read", "queries_quota": 50000, "queries_used": 123},
	}
	workspaceJSON(t, reopened.client, http.MethodPut, "/api/v1/tenants/"+tenantID, map[string]any{"metadata": paid}, 200)
	if _, err := reopened.findOrCreateOAuthUser("google", "synthetic-owner", "owner@example.test", "Owner", ""); err != nil {
		t.Fatal(err)
	}
	current := workspaceJSON(t, reopened.client, http.MethodGet, "/api/v1/tenants/"+tenantID, nil, 200)
	got := workspaceBytes(t, current["metadata"])
	want := workspaceBytes(t, paid)
	if !bytes.Equal(got, want) {
		t.Fatal("returning login reset paid metadata")
	}

	// An email match across different authenticated provider IDs is not owner
	// proof. The second identity receives a separate newly initialized tenant.
	other, err := reopened.findOrCreateOAuthUser("github", "different-provider", "owner@example.test", "Other", "")
	if err != nil || other == nil || other.TenantID == tenantID {
		t.Fatalf("provider identities shared a workspace: %v", err)
	}

	// Legacy login behavior stays available, but no ownership is inferred from
	// the email-derived slug and no team record is invented for old accounts.
	legacyID := entities.TenantSlug("legacy@example.test")
	workspaceJSON(t, reopened.client, http.MethodPost, "/api/v1/tenants", map[string]any{"id": legacyID, "name": "Legacy"}, 201)
	legacy, err := reopened.findOrCreateOAuthUser("google", "legacy", "legacy@example.test", "Legacy", "")
	if err != nil || legacy == nil || legacy.TenantID != legacyID || legacy.IsNewUser {
		t.Fatalf("legacy login changed: %v", err)
	}
	if _, found, err := reopened.workspaceConfig(ctx, teamMembersConfigKey(legacyID)); err != nil || found {
		t.Fatal("legacy login invented owner membership")
	}

	// Core admin boundary applies to the distinct conditional route as well.
	developer, err := reopened.authClient.SignAPIKey(tenantID, "synthetic", entities.RoleDeveloper)
	if err != nil {
		t.Fatal(err)
	}
	client := resty.New().SetBaseURL(core.url).SetAuthToken(developer)
	workspaceJSON(t, client, http.MethodPost, "/api/v1/config/conditional/set", map[string]any{"key": "forbidden", "value": 1, "condition": map[string]string{"kind": "absent"}}, 403)
}

func TestOAuthWorkspaceRecoversFailedMemberWrite(t *testing.T) {
	core := newWorkspaceCore(t)
	target, err := url.Parse(core.url)
	if err != nil {
		t.Fatal("invalid loopback URL")
	}
	proxy := httputil.NewSingleHostReverseProxy(target)
	var reject atomic.Bool
	reject.Store(true)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if reject.Load() && r.Method == http.MethodPost && r.URL.Path == "/api/v1/config/conditional/set" {
			// The registry is the first conditional write. Reject the team write
			// after the actual tenant is durable, simulating an interrupted signup.
			var body map[string]json.RawMessage
			if json.NewDecoder(r.Body).Decode(&body) != nil {
				t.Error("invalid request")
				w.WriteHeader(500)
				return
			}
			var key string
			if json.Unmarshal(body["key"], &key) != nil {
				t.Error("invalid key")
				w.WriteHeader(500)
				return
			}
			encoded, err := json.Marshal(body)
			if err != nil {
				t.Error("invalid synthetic body")
				w.WriteHeader(500)
				return
			}
			r.Body = io.NopCloser(bytes.NewReader(encoded))
			r.ContentLength = int64(len(encoded))
			if strings.HasPrefix(key, "team:") {
				w.WriteHeader(503)
				return
			}
		}
		proxy.ServeHTTP(w, r)
	}))
	defer server.Close()
	cp := core.controlPlane(t, server.URL)
	if result, err := cp.findOrCreateOAuthUser("google", "retry", "retry@example.test", "Retry", ""); err == nil || result != nil {
		t.Fatal("uncertain membership write returned a session")
	}
	entry, found, err := cp.workspaceConfig(t.Context(), workspaceRegistryKey("oauth:google:retry"))
	if err != nil || !found {
		t.Fatal("registry not durable before failed member write")
	}
	var registration map[string]any
	if json.Unmarshal(entry, &registration) != nil {
		t.Fatal("invalid registry")
	}
	id, ok := registration["tenant_id"].(string)
	if !ok {
		t.Fatal("missing registered tenant")
	}
	before := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/"+id, nil, 200)
	reject.Store(false)
	core.restart(t)
	result, err := cp.findOrCreateOAuthUser("google", "retry", "retry@example.test", "Retry", "")
	if err != nil || result == nil || result.TenantID != id || result.IsNewUser {
		t.Fatalf("partial signup did not resume its durable workspace: %v", err)
	}
	after := workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/tenants/"+id, nil, 200)
	beforeJSON := workspaceBytes(t, before["metadata"])
	afterJSON := workspaceBytes(t, after["metadata"])
	if !bytes.Equal(beforeJSON, afterJSON) {
		t.Fatal("retry reset trial metadata")
	}
	assertWorkspaceMember(t, cp, id, "oauth:google:retry", roleAdmin)
}

func TestOAuthWorkspaceRequiresConditionalCoreCapability(t *testing.T) {
	if os.Getenv("ALLSOURCE_CORE_BINARY") == "" {
		t.Skip("set ALLSOURCE_CORE_BINARY for actual Core proof")
	}
	core := newWorkspaceCore(t)
	target, err := url.Parse(core.url)
	if err != nil {
		t.Fatal("invalid loopback URL")
	}
	proxy := httputil.NewSingleHostReverseProxy(target)
	var unconditional atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost && r.URL.Path == "/api/v1/config/conditional/set" {
			w.WriteHeader(404)
			return
		}
		if r.Method == http.MethodPost && r.URL.Path == "/api/v1/config" {
			unconditional.Add(1)
		}
		proxy.ServeHTTP(w, r)
	}))
	defer server.Close()
	cp := core.controlPlane(t, server.URL)
	if result, err := cp.findOrCreateOAuthUser("google", "old-core", "old@example.test", "Old", ""); err == nil || result != nil {
		t.Fatal("unsupported conditional writes returned a session")
	}
	if unconditional.Load() != 0 {
		t.Fatal("conditional write fell back to unsafe upsert")
	}
}
