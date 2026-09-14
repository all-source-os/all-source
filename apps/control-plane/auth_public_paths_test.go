package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
)

// TestAuthMiddleware_PublicPathAllowlist locks the global AuthMiddleware's
// public-path skip behavior. It exercises the real middleware (not the
// handler-direct callHandler path the delegation tests use), because the
// Gap-1 anonymous-trial regression lived precisely in this layer: the
// endpoint is unauthenticated by design, but the middleware allowlist
// omitted it, so every unauthenticated call was aborted with 401 before it
// ever reached the handler.
//
// authClient is nil deliberately: every case here sends NO Authorization
// header, so a non-public path fails at ExtractToken (before authClient is
// touched) and a public path returns at c.Next() (also before authClient is
// touched). If a future change dereferences authClient earlier, this test
// will panic — which is the signal we want.
func TestAuthMiddleware_PublicPathAllowlist(t *testing.T) {
	gin.SetMode(gin.TestMode)

	cases := []struct {
		name       string
		method     string
		path       string
		wantPassed bool // true => middleware called c.Next() (no auth required)
	}{
		{"anonymous-trial mint is public", http.MethodPost, "/api/v1/agents/anonymous-trial", true},
		{"trial claim stays auth-gated", http.MethodPost, "/api/v1/agents/claim", false},
		{"agent register stays auth-gated", http.MethodPost, "/api/v1/agents/register", false},
		{"agents prefix is not blanket-public", http.MethodGet, "/api/v1/agents/me/payments", false},
		{"health is public", http.MethodGet, pathHealth, true},
		{"onboard prefix is public", http.MethodPost, "/api/v1/onboard/start", true},
		{"design partner application is public", http.MethodPost, "/api/v1/design-partners/applications", true},
		{"design partner list is not public", http.MethodGet, "/api/v1/design-partners/applications", false},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			w := httptest.NewRecorder()
			c, engine := gin.CreateTestContext(w)

			passed := false
			engine.Use(AuthMiddleware(nil))
			engine.Handle(tc.method, tc.path, func(c *gin.Context) {
				passed = true
				c.Status(http.StatusOK)
			})

			c.Request = httptest.NewRequestWithContext(context.Background(), tc.method, tc.path, http.NoBody)
			engine.ServeHTTP(w, c.Request)

			if passed != tc.wantPassed {
				t.Fatalf("%s %s: reached handler = %v, want %v (status %d)",
					tc.method, tc.path, passed, tc.wantPassed, w.Code)
			}
			if !tc.wantPassed && w.Code != http.StatusUnauthorized {
				t.Fatalf("%s %s: expected 401 for auth-gated path, got %d",
					tc.method, tc.path, w.Code)
			}
		})
	}
}
