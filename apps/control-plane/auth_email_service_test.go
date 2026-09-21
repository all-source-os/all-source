package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/go-resty/resty/v2"
)

func TestEmailAuthService(t *testing.T) {
	for _, tc := range []struct {
		name                     string
		signup                   bool
		authStatus, tenantStatus int
		invalid                  bool
		want                     int
	}{
		{"signup", true, 200, 201, false, 201},
		{"returning-login", false, 200, 409, false, 200},
		{"resume-workspace", false, 200, 201, false, 200},
		{"wrong-password", false, 401, 0, false, 401},
		{"duplicate", true, 422, 0, false, 422},
		{"upstream-failure", true, 500, 0, false, 502},
		{"missing-identity", true, 200, 0, true, 502},
		{"tenant-failure", true, 200, 500, false, 502},
	} {
		t.Run(tc.name, func(t *testing.T) {
			t.Setenv("ADMIN_EMAILS", "owner@example.test")
			auth := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				path := "/api/auth/sign-in/email"
				if tc.signup {
					path = "/api/auth/sign-up/email"
				}
				if r.URL.Path != path || r.Method != "POST" {
					t.Errorf("unexpected auth request %s %s", r.Method, r.URL.Path)
				}
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(tc.authStatus)
				if tc.invalid {
					_, _ = w.Write([]byte(`{"token":"opaque"}`))
					return
				}
				_, _ = w.Write([]byte(`{"token":"opaque","user":{"id":"user-123","email":"owner@example.test","name":"Example"}}`))
			}))
			defer auth.Close()
			t.Setenv("AUTH_SERVICE_URL", auth.URL)
			calls := 0
			core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				calls++
				if r.URL.Path == "/api/v1/tenants/email-user-123" {
					if r.Method == http.MethodPut {
						var update struct {
							Metadata map[string]interface{} `json:"metadata"`
						}
						if err := json.NewDecoder(r.Body).Decode(&update); err != nil {
							t.Fatal(err)
						}
						for _, key := range []string{"quota", "quotas"} {
							q, ok := update.Metadata[key].(map[string]interface{})
							if !ok || q["events_quota"] != float64(1000) || q["queries_quota"] != float64(100) {
								t.Errorf("missing trial limits in %s", key)
							}
						}
					}
					w.Header().Set("Content-Type", "application/json")
					_, _ = w.Write([]byte(`{"metadata":{"subscription":{"status":"active"}}}`))
					return
				}
				if r.URL.Path != "/api/v1/tenants" {
					t.Errorf("unexpected Core path %s", r.URL.Path)
				}
				var body map[string]interface{}
				_ = json.NewDecoder(r.Body).Decode(&body)
				if body["id"] != "email-user-123" {
					t.Errorf("must scope tenant to authenticated identity")
				}
				w.WriteHeader(tc.tenantStatus)
				_, _ = w.Write([]byte(`{}`))
			}))
			defer core.Close()
			cp := &ControlPlane{client: resty.New().SetBaseURL(core.URL), authClient: NewAuthClient("test-secret", core.URL)}
			router := gin.New()
			router.POST("/register", cp.RegisterHandler)
			router.POST("/login", cp.LoginHandler)
			path := "/login"
			if tc.signup {
				path = "/register"
			}
			req := httptest.NewRequest("POST", path, strings.NewReader(`{"name":"Example","email":"owner@example.test","password":"TestPassword123!"}`))
			req.Header.Set("Content-Type", "application/json")
			w := httptest.NewRecorder()
			router.ServeHTTP(w, req)
			if w.Code != tc.want {
				t.Fatalf("status %d want %d: %s", w.Code, tc.want, w.Body.String())
			}
			if tc.authStatus != 200 || tc.invalid {
				if calls != 0 {
					t.Fatal("workspace touched after failed authentication")
				}
				return
			}
			if tc.want < 300 {
				var data struct {
					Token   string `json:"token"`
					NewUser bool   `json:"new_user"`
				}
				_ = json.Unmarshal(w.Body.Bytes(), &data)
				claims, err := cp.authClient.ValidateToken(data.Token)
				if err != nil {
					t.Fatal(err)
				}
				if claims.TenantID != "email-user-123" || claims.UserID != "user-123" || string(claims.Role) != "developer" {
					t.Fatalf("incorrect identity scope or privilege: %+v", claims)
				}
				if data.NewUser != (tc.tenantStatus == 201) {
					t.Fatal("wrong onboarding flag")
				}
			}
		})
	}
}
