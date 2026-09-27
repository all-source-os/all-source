package main

import (
	"net/http"
	"net/http/httptest"
	"net/http/httputil"
	"net/url"
	"strings"
	"sync"
	"sync/atomic"
	"testing"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func invitationOwner(t *testing.T, cp *ControlPlane) *AuthContext {
	t.Helper()
	result, err := cp.findOrCreateOAuthUser("google", "inviter", "inviter@example.test", "Inviter", "")
	if err != nil {
		t.Fatal("synthetic owner setup failed")
	}
	return &AuthContext{UserID: result.UserID, TenantID: result.TenantID, Role: entities.RoleDeveloper}
}

func TestTeamInvitationRealCore(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	actor := invitationOwner(t, cp)
	token, _, err := cp.createTeamInvitation(t.Context(), actor, "invited@example.test", roleMember)
	if err != nil {
		t.Fatal("current tenant administrator could not issue invitation")
	}
	value, found, err := cp.workspaceConfig(t.Context(), teamMembersConfigKey(actor.TenantID))
	if err != nil || !found || strings.Contains(string(value), token) {
		t.Fatal("invitation plaintext appeared in team state")
	}
	identity := &providerUserInfo{ProviderID: "invitee", Email: "invited@example.test", Name: "Invited"}
	if result, err := cp.completeOAuthSignIn("google", identity, token); err == nil || result != nil {
		t.Fatal("unverified email accepted invitation")
	}
	identity.EmailVerified = true
	identity.Email = "wrong@example.test"
	if result, err := cp.completeOAuthSignIn("google", identity, token); err == nil || result != nil {
		t.Fatal("wrong email accepted invitation")
	}
	identity.Email = "invited@example.test"
	accepted, err := cp.completeOAuthSignIn("google", identity, token)
	if err != nil || accepted == nil || accepted.TenantID != actor.TenantID {
		t.Fatalf("valid invitation did not return invited workspace: %v", err)
	}
	core.restart(t)
	returned, err := cp.findOrCreateOAuthUser("google", "invitee", "invited@example.test", "Invited", "")
	if err != nil || returned == nil || returned.TenantID != actor.TenantID || returned.IsNewUser {
		t.Fatal("returning invitee lost durable workspace selection")
	}
	workspaceJSON(t, cp.client, http.MethodGet, "/api/v1/config/"+workspaceRegistryKey(accepted.UserID), nil, 404)
	snapshot, err := cp.readTeamSnapshot(t.Context(), actor.TenantID)
	if err != nil || snapshot.State.Invitations[teamInviteDigest(token)].AcceptedBy != accepted.UserID || teamRole(snapshot.State, accepted.UserID) != roleMember {
		t.Fatal("admission and consumption were not durably stored together")
	}
	if err := cp.editTeam(t.Context(), actor, func(state *teamState) error {
		return removeTeamMember(state, accepted.UserID)
	}); err != nil {
		t.Fatal("could not remove synthetic invitee")
	}
	if result, err := cp.completeOAuthSignIn("google", identity, token); err == nil || result != nil {
		t.Fatal("consumed invitation restored removed member")
	}
	if result, err := cp.findOrCreateOAuthUser("google", "invitee", "invited@example.test", "Invited", ""); err == nil || result != nil {
		t.Fatal("removed invitee obtained returning session")
	}
	newToken, _, err := cp.createTeamInvitation(t.Context(), actor, identity.Email, roleMember)
	if err != nil {
		t.Fatal("could not issue fresh invitation")
	}
	if result, err := cp.completeOAuthSignIn("google", identity, newToken); err != nil || result == nil || result.TenantID != actor.TenantID {
		t.Fatal("explicit fresh invitation could not restore intended member")
	}
}

func TestTeamInvitationConcurrentIdentitiesHaveOneWinner(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	actor := invitationOwner(t, cp)
	token, _, err := cp.createTeamInvitation(t.Context(), actor, "shared@example.test", roleMember)
	if err != nil {
		t.Fatal(err)
	}
	var successes atomic.Int32
	var callers sync.WaitGroup
	for _, provider := range []string{"google", "github"} {
		callers.Go(func() {
			result, err := cp.completeOAuthSignIn(provider, &providerUserInfo{
				ProviderID: "same-email-different-subject", Email: "shared@example.test", Name: "Shared", EmailVerified: true,
			}, token)
			if err == nil && result != nil {
				successes.Add(1)
			}
		})
	}
	callers.Wait()
	snapshot, err := cp.readTeamSnapshot(t.Context(), actor.TenantID)
	if err != nil || successes.Load() != 1 || len(snapshot.State.Members) != 2 {
		t.Fatal("single invitation admitted multiple provider identities")
	}
}

func TestTeamInvitationFailedSelectionCannotRestoreRemovedMember(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	actor := invitationOwner(t, cp)
	token, _, err := cp.createTeamInvitation(t.Context(), actor, "retry@example.test", roleMember)
	if err != nil {
		t.Fatal(err)
	}
	target, err := url.Parse(core.url)
	if err != nil {
		t.Fatal("invalid loopback URL")
	}
	proxy := httputil.NewSingleHostReverseProxy(target)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodPost && r.URL.Path == "/api/v1/config" {
			w.WriteHeader(503)
			return
		}
		proxy.ServeHTTP(w, r)
	}))
	defer server.Close()
	failed := core.controlPlane(t, server.URL)
	identity := &providerUserInfo{ProviderID: "retry", Email: "retry@example.test", Name: "Retry", EmailVerified: true}
	if result, err := failed.completeOAuthSignIn("google", identity, token); err == nil || result != nil {
		t.Fatal("failed workspace selection returned a session")
	}
	core.restart(t)
	snapshot, err := cp.readTeamSnapshot(t.Context(), actor.TenantID)
	if err != nil || teamRole(snapshot.State, "oauth:google:retry") != roleMember {
		t.Fatal("successful admission vanished after failed selection and restart")
	}
	if err := cp.editTeam(t.Context(), actor, func(state *teamState) error {
		return removeTeamMember(state, "oauth:google:retry")
	}); err != nil {
		t.Fatal(err)
	}
	if result, err := cp.completeOAuthSignIn("google", identity, token); err == nil || result != nil {
		t.Fatal("retry of partial acceptance resurrected removed member")
	}
}
