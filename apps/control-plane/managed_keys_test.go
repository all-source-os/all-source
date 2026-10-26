package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func lifecycleClaims() *Claims {
	return &Claims{UserID: "proof-key", TenantID: "proof-tenant", Name: "synthetic", IsAPIKey: true,
		Role: entities.RoleServiceAccount, StandardClaims: jwt.StandardClaims{
			Issuer: "allsource", IssuedAt: time.Now().Unix(), ExpiresAt: time.Now().Add(time.Hour).Unix()}}
}

func lifecycleCreated() map[string]any {
	return map[string]any{"id": "created", "entity_id": "apikey:proof-tenant", "tenant_id": "proof-tenant", "event_type": "apikey.created",
		"timestamp": "2026-01-01T00:00:00Z", "version": 1, "payload": map[string]any{"key": map[string]any{
			"id": "proof-key", "tenant_id": "proof-tenant", "active": true, "scopes": []string{"events:read", "events:write"}, "expires_at": nil}}}
}

func lifecyclePage(events ...map[string]any) map[string]any {
	return map[string]any{"events": events, "count": len(events), "total_count": len(events), "has_more": false, "entity_version": len(events),
		"archive_integrity": map[string]any{"protocol": "retained-entity-v1", "tenant_id": "proof-tenant", "entity_id": "apikey:proof-tenant"}}
}

func lifecycleRequest(t *testing.T, client *AuthClient, claims *Claims, method, path, body string) *httptest.ResponseRecorder {
	t.Helper()
	token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(client.jwtSecret))
	if err != nil {
		t.Fatal(err)
	}
	router := gin.New()
	router.Use(AuthMiddleware(client))
	router.Any("/*path", func(c *gin.Context) { c.Status(http.StatusOK) })
	req := httptest.NewRequestWithContext(t.Context(), method, path, strings.NewReader(body))
	req.Header.Set("Authorization", "Bearer "+token)
	response := httptest.NewRecorder()
	router.ServeHTTP(response, req)
	return response
}

func TestManagedKeyRevokeAtIngress(t *testing.T) {
	gin.SetMode(gin.TestMode)
	page := lifecyclePage(lifecycleCreated())
	var mu sync.Mutex
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		defer mu.Unlock()
		if r.URL.Query().Get("integrity") != "retained-entity-v1" || r.URL.Query().Get("entity_id") != "apikey:proof-tenant" || r.Header.Get("Authorization") == "" {
			t.Error("missing authoritative tenant query or internal authorization")
		}
		if err := json.NewEncoder(w).Encode(page); err != nil {
			t.Error(err)
		}
	}))
	defer core.Close()
	client := NewAuthClient("synthetic-secret", core.URL)
	claims := lifecycleClaims()
	if result := lifecycleRequest(t, client, claims, "GET", "/api/v1/events/query", ""); result.Code != 200 {
		t.Fatalf("active key: %d %s", result.Code, result.Body)
	}
	revoked := map[string]any{"id": "revoked", "tenant_id": "proof-tenant", "entity_id": "apikey:proof-tenant", "event_type": "apikey.revoked", "version": 2,
		"timestamp": "2026-01-01T00:00:01Z", "payload": map[string]any{"key_id": "proof-key"}}
	mu.Lock()
	page = lifecyclePage(lifecycleCreated(), revoked)
	mu.Unlock()
	if result := lifecycleRequest(t, client, claims, "GET", "/api/v1/events/query", ""); result.Code != 401 {
		t.Fatalf("revoked key: %d %s", result.Code, result.Body)
	}
}

func TestManagedKeyAuthorityFailsClosed(t *testing.T) {
	cases := map[string]func(map[string]any, map[string]any){
		"unknown": func(page, _ map[string]any) {
			page["events"] = []any{}
			page["count"] = 0
			page["total_count"] = 0
			page["entity_version"] = 0
		},
		"truncated":         func(page, _ map[string]any) { page["has_more"] = true; page["total_count"] = 1002 },
		"missing integrity": func(page, _ map[string]any) { delete(page, "archive_integrity") },
		"cross tenant":      func(_, event map[string]any) { event["tenant_id"] = "another-tenant" },
		"missing version":   func(page, _ map[string]any) { delete(page, "entity_version") },
		"wrong version":     func(page, _ map[string]any) { page["entity_version"] = 2 },
		"wrong count":       func(page, _ map[string]any) { page["count"] = 0 },
		"expired": func(_, event map[string]any) {
			lifecycleTestRecord(t, event)["expires_at"] = "2020-01-01T00:00:00Z"
		},
		"unknown scope": func(_, event map[string]any) {
			lifecycleTestRecord(t, event)["scopes"] = []string{"admin"}
		},
		"empty scopes": func(_, event map[string]any) {
			lifecycleTestRecord(t, event)["scopes"] = []string{}
		},
	}
	for name, change := range cases {
		t.Run(name, func(t *testing.T) {
			event := lifecycleCreated()
			page := lifecyclePage(event)
			change(page, event)
			core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				if err := json.NewEncoder(w).Encode(page); err != nil {
					t.Error(err)
				}
			}))
			defer core.Close()
			result := lifecycleRequest(t, NewAuthClient("synthetic-secret", core.URL), lifecycleClaims(), "GET", "/api/v1/events/query", "")
			if result.Code != 401 {
				t.Fatalf("expected401, got%d %s", result.Code, result.Body)
			}
		})
	}
	t.Run("unavailable", func(t *testing.T) {
		core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(503) }))
		defer core.Close()
		if result := lifecycleRequest(t, NewAuthClient("synthetic-secret", core.URL), lifecycleClaims(), "GET", "/api/v1/events/query", ""); result.Code != 401 {
			t.Fatal(result.Code)
		}
	})
}

func TestManagedKeyOperationScopes(t *testing.T) {
	event := lifecycleCreated()
	lifecycleTestRecord(t, event)["scopes"] = []string{"events:read"}
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		if err := json.NewEncoder(w).Encode(lifecyclePage(event)); err != nil {
			t.Error(err)
		}
	}))
	defer core.Close()
	client := NewAuthClient("synthetic-secret", core.URL)
	for _, tc := range []struct {
		method, path, body string
		status             int
	}{
		{"GET", "/api/v1/prime/nodes/proof", "", 200},
		{"POST", "/api/v1/events", `{}`, 403},
		{"POST", "/api/v1/prime/nodes", `{}`, 403},
		{"GET", "/api/v1/teams/agent-keys", "", 403},
		{"POST", "/api/v1/prime/mcp", `{"method":"tools/call","params":{"name":"prime_stats"}}`, 200},
		{"POST", "/api/v1/prime/mcp", `{"method":"tools/call","params":{"name":"prime_add_node"}}`, 403},
		{"POST", "/api/v1/prime/mcp", `{"method":"tools/call","params":{"name":"unknown"}}`, 403},
		{"POST", "/api/v1/prime/mcp", `[{"method":"tools/call","params":{"name":"prime_stats"}}]`, 403},
	} {
		t.Run(tc.path+tc.body, func(t *testing.T) {
			if result := lifecycleRequest(t, client, lifecycleClaims(), tc.method, tc.path, tc.body); result.Code != tc.status {
				t.Fatalf("got%d, want%d: %s", result.Code, tc.status, result.Body)
			}
		})
	}
}

func lifecycleTestRecord(t *testing.T, event map[string]any) map[string]any {
	t.Helper()
	payload, ok := event["payload"].(map[string]any)
	if !ok {
		t.Fatal("invalid test payload")
	}
	key, ok := payload["key"].(map[string]any)
	if !ok {
		t.Fatal("invalid test key")
	}
	return key
}

func TestCredentialKindCompatibility(t *testing.T) {
	client := NewAuthClient("synthetic-secret", "")
	human := lifecycleClaims()
	human.IsAPIKey = false
	human.Role = entities.RoleDeveloper
	if result := lifecycleRequest(t, client, human, "GET", "/api/v1/events/query", ""); result.Code != 200 {
		t.Fatal(result.Code)
	}
	markerless := lifecycleClaims()
	markerless.IsAPIKey = false
	if result := lifecycleRequest(t, client, markerless, "GET", queryEventsPath, ""); result.Code != 401 {
		t.Fatal("markerless serviceaccount bypassed lifecycle")
	}
	for _, kind := range []string{"", delegationKind} {
		claims := lifecycleClaims()
		claims.Name = ""
		claims.CredentialKind = kind
		claims.ExpiresAt = claims.IssuedAt + 60
		if result := lifecycleRequest(t, client, claims, "GET", "/api/v1/events/query", ""); result.Code != 200 {
			t.Fatalf("delegation %q: %d", kind, result.Code)
		}
		claims.ExpiresAt = claims.IssuedAt + 3600
		if result := lifecycleRequest(t, client, claims, "GET", "/api/v1/events/query", ""); result.Code != 401 {
			t.Fatalf("long-lived bypass %q: %d", kind, result.Code)
		}
	}
}

func TestManagedKeyDuplicateRegistrationAndIsolation(t *testing.T) {
	created := lifecycleCreated()
	duplicate := lifecycleCreated()
	duplicate["id"] = "duplicate"
	duplicate["version"] = 2
	other := lifecycleCreated()
	other["id"] = "other"
	other["version"] = 3
	key := lifecycleTestRecord(t, other)
	key["id"] = "other-key"
	key["scopes"] = []string{}
	page := lifecyclePage(created, duplicate, other)
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		if err := json.NewEncoder(w).Encode(page); err != nil {
			t.Error(err)
		}
	}))
	defer core.Close()
	client := NewAuthClient("synthetic-secret", core.URL)
	if response := lifecycleRequest(t, client, lifecycleClaims(), "GET", queryEventsPath, ""); response.Code != 200 {
		t.Fatalf("idempotent duplicate or unrelated invalid scope blocked valid key: %d %s", response.Code, response.Body)
	}
}

func TestDelegationClockSkewAndExpiry(t *testing.T) {
	client := NewAuthClient("synthetic-secret", "")
	for _, offset := range []int64{3, 30} {
		claims := lifecycleClaims()
		claims.Name = ""
		claims.CredentialKind = delegationKind
		claims.IssuedAt = time.Now().Unix() + offset
		claims.ExpiresAt = claims.IssuedAt + 60
		want := 401
		if offset == 3 {
			want = 200
		}
		if result := lifecycleRequest(t, client, claims, "GET", queryEventsPath, ""); result.Code != want {
			t.Fatalf("skew%d: want%d got%d", offset, want, result.Code)
		}
	}
	claims := lifecycleClaims()
	claims.IsAPIKey = false
	claims.Role = entities.RoleDeveloper
	claims.ExpiresAt = time.Now().Unix()
	if result := lifecycleRequest(t, client, claims, "GET", queryEventsPath, ""); result.Code != 401 {
		t.Fatal("expiry boundary accepted")
	}
}

func TestManagedKeyScopesCannotExpandReadonlyRole(t *testing.T) {
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		if err := json.NewEncoder(w).Encode(lifecyclePage(lifecycleCreated())); err != nil {
			t.Error(err)
		}
	}))
	defer core.Close()
	client := NewAuthClient("synthetic-secret", core.URL)
	claims := lifecycleClaims()
	claims.Role = entities.RoleReadOnly
	if response := lifecycleRequest(t, client, claims, "POST", "/api/v1/prime/mcp", `{"method":"tools/call","params":{"name":"prime_add_node"}}`); response.Code != 403 {
		t.Fatalf("readonly role expanded: %d", response.Code)
	}
	if response := lifecycleRequest(t, client, claims, "GET", "/api/v1/prime/nodes/proof", ""); response.Code != 200 {
		t.Fatal(response.Code)
	}
}

func TestManagedAdvertisedScopesPreserveRoleAndOperationBounds(t *testing.T) {
	event := lifecycleCreated()
	lifecycleTestRecord(t, event)["scopes"] = []string{"events:read", "events:write", "queries:execute", "projections:read", "projections:write", "schemas:read", "schemas:write"}
	var mu sync.Mutex
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		mu.Lock()
		defer mu.Unlock()
		if err := json.NewEncoder(w).Encode(lifecyclePage(event)); err != nil {
			t.Error(err)
		}
	}))
	defer core.Close()
	client := NewAuthClient("synthetic-secret", core.URL)
	claims := lifecycleClaims()
	claims.Role = entities.RoleDeveloper
	for _, request := range []struct{ method, path string }{
		{"GET", queryEventsPath}, {"GET", "/api/v1/schemas"}, {"POST", "/api/v1/schemas/validate"}, {"POST", "/api/v1/schemas"}, {"PUT", "/api/v1/read-models/synthetic"},
	} {
		if response := lifecycleRequest(t, client, claims, request.method, request.path, ""); response.Code != 200 {
			t.Fatalf("advertised scope denied %s %s: %d", request.method, request.path, response.Code)
		}
	}
	claims.Role = entities.RoleReadOnly
	if response := lifecycleRequest(t, client, claims, "PUT", "/api/v1/read-models/synthetic", ""); response.Code != 403 {
		t.Fatal("projection scope expanded readonly role")
	}
	if response := lifecycleRequest(t, client, claims, "POST", "/api/v1/schemas", ""); response.Code != 403 {
		t.Fatal("schema scope expanded readonly role")
	}
	if response := lifecycleRequest(t, client, claims, "POST", "/api/v1/schemas/validate", ""); response.Code != 200 {
		t.Fatal("readonly schema validation denied")
	}
	claims.Role = entities.RoleServiceAccount
	if response := lifecycleRequest(t, client, claims, "POST", "/api/v1/schemas", ""); response.Code != 403 {
		t.Fatal("schema scope expanded serviceaccount governance role")
	}
	mu.Lock()
	lifecycleTestRecord(t, event)["scopes"] = []string{"schemas:read"}
	mu.Unlock()
	claims.Role = entities.RoleDeveloper
	if response := lifecycleRequest(t, client, claims, "POST", "/api/v1/schemas", ""); response.Code != 403 {
		t.Fatal("read scope permitted schema registration")
	}
}

func TestManagedNonExpiringKeysStillRequireRegistration(t *testing.T) {
	page := lifecyclePage(lifecycleCreated())
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		if err := json.NewEncoder(w).Encode(page); err != nil {
			t.Error(err)
		}
	}))
	defer core.Close()
	client := NewAuthClient("synthetic-secret", core.URL)
	claims := lifecycleClaims()
	claims.CredentialKind = managedKeyKind
	claims.ExpiresAt = 0
	if response := lifecycleRequest(t, client, claims, "GET", queryEventsPath, ""); response.Code != 200 {
		t.Fatalf("registered non-expiring key: %d %s", response.Code, response.Body)
	}
	claims.UserID = "unregistered"
	if response := lifecycleRequest(t, client, claims, "GET", queryEventsPath, ""); response.Code != 401 {
		t.Fatal("unknown non-expiring key accepted")
	}
}

func TestManagedExpiryCannotUseNullOrZero(t *testing.T) {
	client := NewAuthClient("synthetic-secret", "")
	for _, expires := range []any{nil, int64(0), "bad"} {
		claims := jwt.MapClaims{"sub": "proof-key", "tenant_id": "proof-tenant", "role": "serviceaccount", "is_api_key": true, "credential_kind": managedKeyKind, "exp": expires}
		token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(client.jwtSecret))
		if err != nil {
			t.Fatal(err)
		}
		if _, err := client.ValidateToken(token); err == nil {
			t.Fatalf("malformed expiry accepted: %v", expires)
		}
	}
}

func TestNonExpiringJWTRequiresNonExpiringRecord(t *testing.T) {
	event := lifecycleCreated()
	lifecycleTestRecord(t, event)["expires_at"] = time.Now().Add(time.Hour).UTC().Format(time.RFC3339)
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		if err := json.NewEncoder(w).Encode(lifecyclePage(event)); err != nil {
			t.Error(err)
		}
	}))
	defer core.Close()
	claims := lifecycleClaims()
	claims.CredentialKind = managedKeyKind
	claims.ExpiresAt = 0
	if result := lifecycleRequest(t, NewAuthClient("synthetic-secret", core.URL), claims, "GET", queryEventsPath, ""); result.Code != 401 {
		t.Fatal("finite record accepted non-expiring token")
	}
}
