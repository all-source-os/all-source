package main

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func TestTeamMembershipRealCore(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	const tenant = "team-proof"
	members := []TeamMember{
		{UserID: "operator", Role: roleAdmin},
		{UserID: "second-operator", Role: roleAdmin},
		{UserID: "member", Role: roleMember},
	}
	setWorkspaceConfig(t, cp, teamMembersConfigKey(tenant), members)
	// Global developer is sufficient when the current tenant grants team admin.
	actor := &AuthContext{UserID: "operator", TenantID: tenant, Role: entities.RoleDeveloper}
	router := membershipRouter(cp, actor)
	membershipRequest(t, router, http.MethodPut, "/members/member", `{"role":"admin"}`, 200)
	snapshot, err := cp.readTeamSnapshot(t.Context(), tenant)
	if err != nil || teamRole(snapshot.State, "member") != roleAdmin {
		t.Fatal("current tenant administrator could not promote member")
	}
	value, found, err := cp.workspaceConfig(t.Context(), teamMembersConfigKey(tenant))
	if err != nil || !found || !strings.Contains(string(value), `"schema_version":2`) {
		t.Fatal("legacy list was not upgraded by successful conditional edit")
	}
	core.restart(t)
	snapshot, err = cp.readTeamSnapshot(t.Context(), tenant)
	if err != nil || teamRole(snapshot.State, "member") != roleAdmin {
		t.Fatal("member change did not survive Core restart")
	}

	// An agent API key cannot impersonate its subject for team administration.
	keyActor := *actor
	keyActor.IsAPIKey = true
	membershipRequest(t, membershipRouter(cp, &keyActor), http.MethodPut, "/members/member", `{"role":"member"}`, 403)
	// Global admin does not confer membership or authority inside another team.
	outsider := &AuthContext{UserID: "outsider", TenantID: tenant, Role: entities.RoleAdmin}
	membershipRequest(t, membershipRouter(cp, outsider), http.MethodGet, "/members", "", 403)
	membershipRequest(t, membershipRouter(cp, outsider), http.MethodDelete, "/members/member", "", 403)
	membershipRequest(t, router, http.MethodDelete, "/members/member", "", 204)
	membershipRequest(t, router, http.MethodDelete, "/members/second-operator", "", 204)
	membershipRequest(t, router, http.MethodPut, "/members/operator", `{"role":"member"}`, 409)
	read := membershipRequest(t, router, http.MethodGet, "/members", "", 200)
	if !strings.Contains(read, `"can_manage":true`) || !strings.Contains(read, `"seats_used":1`) {
		t.Fatal("member listing did not reflect current authority and actual membership")
	}
	if err := cp.editTeam(t.Context(), outsider, func(*teamState) error { return nil }); !errors.Is(err, errTeamForbidden) {
		t.Fatal("team write trusted an absent caller")
	}
}

func TestTeamMembershipRechecksAuthorityAfterConflict(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	const tenant = "team-conflict"
	setWorkspaceConfig(t, cp, teamMembersConfigKey(tenant), []TeamMember{
		{UserID: "operator", Role: roleAdmin}, {UserID: "second", Role: roleAdmin}, {UserID: "member", Role: roleMember},
	})
	target, err := url.Parse(core.url)
	if err != nil {
		t.Fatal("invalid loopback URL")
	}
	proxy := httputil.NewSingleHostReverseProxy(target)
	arrived, release := make(chan struct{}), make(chan struct{})
	var waiting atomic.Bool
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost && r.URL.Path == "/api/v1/config/conditional/set" && waiting.CompareAndSwap(false, true) {
			close(arrived)
			select {
			case <-release:
			case <-r.Context().Done():
				return
			case <-time.After(5 * time.Second):
				w.WriteHeader(503)
				return
			}
		}
		proxy.ServeHTTP(w, r)
	}))
	defer server.Close()
	slow := core.controlPlane(t, server.URL)
	done := make(chan error, 1)
	go func() {
		done <- slow.editTeam(t.Context(), &AuthContext{UserID: "operator", TenantID: tenant}, func(state *teamState) error {
			return changeTeamRole(state, "member", roleAdmin)
		})
	}()
	select {
	case <-arrived:
	case <-time.After(5 * time.Second):
		close(release)
		t.Fatal("delayed edit did not reach conditional boundary")
	}
	err = cp.editTeam(t.Context(), &AuthContext{UserID: "second", TenantID: tenant}, func(state *teamState) error {
		return removeTeamMember(state, "operator")
	})
	close(release)
	if err != nil {
		t.Fatal("concurrent member removal failed")
	}
	select {
	case err := <-done:
		if !errors.Is(err, errTeamForbidden) {
			t.Fatal("stale operator authority survived conflict retry")
		}
	case <-time.After(5 * time.Second):
		t.Fatal("delayed edit did not finish")
	}
	snapshot, err := cp.readTeamSnapshot(t.Context(), tenant)
	if err != nil || teamRole(snapshot.State, "operator") != "" || teamRole(snapshot.State, "member") != roleMember {
		t.Fatal("stale team write restored removal or unauthorized role change")
	}
}

func membershipRouter(cp *ControlPlane, actor *AuthContext) *gin.Engine {
	router := gin.New()
	router.Use(func(c *gin.Context) { c.Set("auth", actor) })
	router.GET("/members", cp.ListMembersHandler)
	router.PUT("/members/:id", cp.UpdateMemberRoleHandler)
	router.DELETE("/members/:id", cp.DeleteMemberHandler)
	return router
}

func membershipRequest(t *testing.T, handler http.Handler, method, path, body string, status int) string {
	t.Helper()
	request := httptest.NewRequestWithContext(t.Context(), method, path, strings.NewReader(body))
	request.Header.Set("Content-Type", "application/json")
	response := httptest.NewRecorder()
	handler.ServeHTTP(response, request)
	if response.Code != status {
		t.Fatalf("membership HTTP status %d, expected %d", response.Code, status)
	}
	return response.Body.String()
}
