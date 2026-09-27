package main

import (
	"net/http/httptest"
	"os"
	"sync"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
)

// Opt-in local browser fixture: actual Core, membership handlers and signed
// sessions. Only identity-provider exchange and Query Service shell responses
// are synthetic. No production credentials, endpoints or external mail calls.
func TestTeamBrowserFixture(t *testing.T) {
	if os.Getenv("ALLSOURCE_TEAM_BROWSER_FIXTURE") != "1" {
		t.Skip("manual browser fixture")
	}
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	router := gin.New()
	stop := make(chan struct{})
	var once sync.Once
	router.POST("/fixture/stop", func(c *gin.Context) { c.Status(204); once.Do(func() { close(stop) }) })
	router.POST("/api/v1/auth/login", func(c *gin.Context) {
		var request struct {
			Email string `json:"email"`
		}
		if c.ShouldBindJSON(&request) != nil || (request.Email != "owner@example.test" && request.Email != "member@example.test") {
			c.Status(403)
			return
		}
		id, name := "browser-owner", "Synthetic owner"
		if request.Email == "member@example.test" {
			id, name = "browser-member", "Synthetic member"
		}
		result, err := cp.completeOAuthSignIn("google", &providerUserInfo{ProviderID: id, Email: request.Email, Name: name, EmailVerified: true}, "")
		if err != nil {
			c.Status(503)
			return
		}
		c.JSON(200, gin.H{"token": result.Token, "new_user": false})
	})
	secured := router.Group("/", AuthMiddleware(cp.authClient))
	secured.GET("/api/auth/me", func(c *gin.Context) {
		actor := mustAuthContext(c)
		c.JSON(200, gin.H{"data": gin.H{"user": gin.H{"id": actor.UserID, "email": actor.Email, "name": actor.Username, "role": "developer"}}})
	})
	secured.GET("/api/tenant", func(c *gin.Context) {
		actor := mustAuthContext(c)
		c.JSON(200, gin.H{"data": gin.H{"id": actor.TenantID, "name": "Synthetic workspace", "subscription": gin.H{"tier": "trial", "status": "active"}, "quotas": gin.H{}}})
	})
	secured.GET("/api/v1/teams/members", cp.ListMembersHandler)
	secured.POST("/api/v1/teams/invite", cp.InviteHandler)
	secured.POST("/api/v1/teams/join", cp.TeamJoinHandler)
	secured.PUT("/api/v1/teams/members/:id", cp.UpdateMemberRoleHandler)
	secured.DELETE("/api/v1/teams/members/:id", cp.DeleteMemberHandler)
	secured.GET("/api/v1/teams/agent-keys", func(c *gin.Context) { c.JSON(200, gin.H{"agent_keys": []any{}}) })
	server := httptest.NewServer(router)
	defer server.Close()
	t.Logf("Synthetic team browser fixture: %s", server.URL)
	select {
	case <-stop:
	case <-time.After(8 * time.Minute):
	case <-t.Context().Done():
	}
	// All test-created processes and temporary storage are closed by Core fixture cleanup.
}
