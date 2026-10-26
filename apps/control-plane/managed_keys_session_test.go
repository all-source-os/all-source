package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"
)

func TestManagedKeyOwnSession(t *testing.T) {
	for _, scenario := range []string{"query-only", "write-only", "readonly", "revoked", "unregistered", "invalid-role", "empty-scopes", "expired"} {
		t.Run(scenario, func(t *testing.T) {
			created := lifecycleCreated()
			page := lifecyclePage(created)
			claims := lifecycleClaims()
			want := http.StatusOK
			switch scenario {
			case "query-only":
				lifecycleTestRecord(t, created)["scopes"] = []string{"queries:execute"}
			case "write-only":
				lifecycleTestRecord(t, created)["scopes"] = []string{"events:write"}
			case "readonly":
				claims.Role = "readonly"
			case "revoked":
				page = lifecyclePage(created, map[string]any{"id": "revoked", "entity_id": "apikey:proof-tenant", "tenant_id": "proof-tenant", "event_type": "apikey.revoked", "version": 2, "timestamp": "2026-01-02T00:00:00Z", "payload": map[string]any{"key_id": "proof-key"}})
				want = http.StatusUnauthorized
			case "unregistered":
				page = lifecyclePage()
				want = http.StatusUnauthorized
			case "invalid-role":
				claims.Role = "unknown"
				want = http.StatusForbidden
			case "empty-scopes":
				lifecycleTestRecord(t, created)["scopes"] = []string{}
				want = http.StatusUnauthorized
			case "expired":
				lifecycleTestRecord(t, created)["expires_at"] = "2020-01-01T00:00:00Z"
				want = http.StatusUnauthorized
			}
			core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				if err := json.NewEncoder(w).Encode(page); err != nil {
					t.Error(err)
				}
			}))
			defer core.Close()
			client := NewAuthClient("synthetic-secret", core.URL)
			cp := &ControlPlane{authClient: client}
			router := gin.New()
			router.Use(AuthMiddleware(client))
			router.GET("/api/v1/auth/session", cp.SessionHandler)
			token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte(client.jwtSecret))
			if err != nil {
				t.Fatal(err)
			}
			request := func(method, path string) *httptest.ResponseRecorder {
				req := httptest.NewRequestWithContext(t.Context(), method, path, http.NoBody)
				req.Header.Set("Authorization", "Bearer "+token)
				response := httptest.NewRecorder()
				router.ServeHTTP(response, req)
				return response
			}
			response := request(http.MethodGet, "/api/v1/auth/session?tenant_id=other-tenant")
			if response.Code != want {
				t.Fatalf("got %d, want %d: %s", response.Code, want, response.Body)
			}
			if want != http.StatusOK {
				return
			}
			var identity struct {
				Tenant string `json:"tenant_id"`
				User   struct {
					ID string `json:"id"`
				} `json:"user"`
			}
			if json.Unmarshal(response.Body.Bytes(), &identity) != nil || identity.Tenant != claims.TenantID || identity.User.ID != claims.UserID {
				t.Fatal("session identity did not stay bound to authenticated claims")
			}
			if result := request(http.MethodGet, "/api/v1/auth/session/other"); result.Code != http.StatusNotFound {
				t.Fatalf("sibling route: %d", result.Code)
			}
			if result := request(http.MethodGet, "/api/v1/teams/agent-keys"); result.Code != http.StatusForbidden {
				t.Fatalf("management route: %d", result.Code)
			}
			if result := request(http.MethodPost, "/api/v1/auth/session"); result.Code != http.StatusForbidden {
				t.Fatalf("session mutation: %d", result.Code)
			}
			if scenario == "query-only" || scenario == "write-only" {
				if result := request(http.MethodGet, queryEventsPath); result.Code != http.StatusForbidden {
					t.Fatalf("session admission expanded event scope: %d", result.Code)
				}
			}
		})
	}
}
