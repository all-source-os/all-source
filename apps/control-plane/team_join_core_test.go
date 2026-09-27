package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"
)

func TestTeamJoinAuthenticatedRealCore(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	owner := invitationOwner(t, cp)
	code, _, err := cp.createTeamInvitation(t.Context(), owner, "join@example.test", roleAdmin)
	if err != nil {
		t.Fatal("invitation setup failed")
	}
	login, err := cp.completeOAuthSignIn("google", &providerUserInfo{ProviderID: "joining", Email: "join@example.test", Name: "Joining", EmailVerified: true}, "")
	if err != nil {
		t.Fatal("sign-in setup failed")
	}
	original, err := cp.authClient.ValidateToken(login.Token)
	if err != nil || !original.EmailVerified {
		t.Fatal("verified identity absent from session")
	}
	router := gin.New()
	router.Use(AuthMiddleware(cp.authClient))
	router.POST("/join", cp.TeamJoinHandler)
	router.GET("/members", cp.ListMembersHandler)
	request := func(token, body string, want int) *httptest.ResponseRecorder {
		t.Helper()
		r := httptest.NewRequestWithContext(t.Context(), http.MethodPost, "/join", strings.NewReader(body))
		r.Header.Set("Authorization", "Bearer "+token)
		r.Header.Set("Content-Type", "application/json")
		w := httptest.NewRecorder()
		router.ServeHTTP(w, r)
		if w.Code != want {
			t.Fatalf("join status %d; want %d", w.Code, want)
		}
		return w
	}
	body, err := json.Marshal(map[string]string{"token": code})
	if err != nil {
		t.Fatal("invitation request encoding failed")
	}
	for _, change := range []func(*Claims){
		func(c *Claims) { c.EmailVerified = false },
		func(c *Claims) { c.IsAPIKey = true },
		func(c *Claims) { c.ViewAs = true },
		func(c *Claims) { c.IsDemo = true },
		func(c *Claims) { c.Email = "other@example.test" },
	} {
		claims := *original
		change(&claims)
		token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, &claims).SignedString([]byte(cp.authClient.jwtSecret))
		if err != nil {
			t.Fatal("synthetic token signing failed")
		}
		request(token, string(body), 403)
	}
	request(login.Token, `{"token":"`+code+`","role":"admin"}`, 400)
	request(login.Token, string(body)+`{}`, 400)
	request(login.Token, `{"token":"`+strings.Repeat("a", 5000)+`"}`, 400)
	w := request(login.Token, string(body), 200)
	if w.Header().Get("Cache-Control") != "no-store" {
		t.Fatal("join response may be cached")
	}
	var result struct {
		Token    string `json:"token"`
		TenantID string `json:"tenant_id"`
	}
	if json.Unmarshal(w.Body.Bytes(), &result) != nil {
		t.Fatal("invalid join response")
	}
	updated, err := cp.authClient.ValidateToken(result.Token)
	if err != nil || result.TenantID != owner.TenantID || updated.TenantID != owner.TenantID || updated.UserID != original.UserID || updated.Role != original.Role || updated.ExpiresAt != original.ExpiresAt {
		t.Fatal("workspace switch changed identity, privilege or expiry")
	}
	r := httptest.NewRequestWithContext(t.Context(), http.MethodGet, "/members", http.NoBody)
	r.Header.Set("Authorization", "Bearer "+result.Token)
	listed := httptest.NewRecorder()
	router.ServeHTTP(listed, r)
	if listed.Code != 200 || !strings.Contains(listed.Body.String(), `"can_manage":true`) {
		t.Fatal("new session did not select admitted team")
	}
}
