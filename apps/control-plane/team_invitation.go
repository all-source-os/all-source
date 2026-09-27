package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"net/mail"
	"regexp"
	"strings"
	"time"
)

var teamInviteTokenPattern = regexp.MustCompile(`^[A-Za-z0-9_-]{32}$`)

const teamInviteLifetime = 7 * 24 * time.Hour

func teamInviteDigest(token string) string {
	hash := sha256.Sum256([]byte(token))
	return hex.EncodeToString(hash[:])
}

func teamInviteLookupKey(token string) string {
	return "team_invite_v2." + teamInviteDigest(token)
}

func (cp *ControlPlane) createTeamInvitation(ctx context.Context, actor *AuthContext, email, role string) (string, teamInvitation, error) {
	if actor == nil || actor.IsAPIKey || actor.ViewAs {
		return "", teamInvitation{}, errTeamForbidden
	}
	email = strings.ToLower(strings.TrimSpace(email))
	address, err := mail.ParseAddress(email)
	if err != nil || address.Address != email || len(email) > 320 || (role != roleAdmin && role != roleMember) {
		return "", teamInvitation{}, errTeamForbidden
	}
	token, err := generateInviteToken()
	if err != nil {
		return "", teamInvitation{}, errTeamUnavailable
	}
	digest := teamInviteDigest(token)
	now := time.Now().UTC()
	invitation := teamInvitation{
		Email: email, Role: role, InvitedBy: actor.UserID,
		CreatedAt: now.Format(time.RFC3339Nano), ExpiresAt: now.Add(teamInviteLifetime).Format(time.RFC3339Nano),
	}
	err = cp.editTeam(ctx, actor, func(state *teamState) error {
		// Expired receipts cannot admit anyone, so they can leave the current
		// snapshot. Their immutable Core history is not claimed to be erased.
		for key, existing := range state.Invitations {
			deadline, err := time.Parse(time.RFC3339Nano, existing.ExpiresAt)
			if err == nil && !now.Before(deadline) {
				delete(state.Invitations, key)
			}
		}
		if len(state.Invitations) >= 128 {
			return errTeamConflict
		}
		state.Invitations[digest] = invitation
		return nil
	})
	if err != nil {
		return "", teamInvitation{}, err
	}
	// This non-authoritative pointer is only for locating the team. The actual
	// invitation and its single-use admission receipt remain in the team record.
	err = cp.createWorkspaceConfig(ctx, teamInviteLookupKey(token), map[string]any{
		"version": 2, "tenant_id": actor.TenantID,
	}, actor.UserID)
	if err != nil {
		return "", teamInvitation{}, err
	}
	resolvedTenant, _, _, err := cp.lookupTeamInvitation(ctx, token)
	if err != nil || resolvedTenant != actor.TenantID {
		return "", teamInvitation{}, errTeamUnavailable
	}
	return token, invitation, nil
}

func (cp *ControlPlane) lookupTeamInvitation(ctx context.Context, token string) (string, teamSnapshot, teamInvitation, error) {
	if !teamInviteTokenPattern.MatchString(token) {
		return "", teamSnapshot{}, teamInvitation{}, errTeamInvitationDenied
	}
	value, found, err := cp.workspaceConfig(ctx, teamInviteLookupKey(token))
	if err != nil {
		return "", teamSnapshot{}, teamInvitation{}, errTeamUnavailable
	}
	if !found {
		return "", teamSnapshot{}, teamInvitation{}, errTeamInvitationDenied
	}
	var pointer struct {
		Version int    `json:"version"`
		Tenant  string `json:"tenant_id"`
	}
	if json.Unmarshal(value, &pointer) != nil || pointer.Version != 2 || !teamTenantPattern.MatchString(pointer.Tenant) {
		return "", teamSnapshot{}, teamInvitation{}, errTeamInvitationDenied
	}
	snapshot, err := cp.readTeamSnapshot(ctx, pointer.Tenant)
	if err != nil {
		return "", teamSnapshot{}, teamInvitation{}, err
	}
	invitation, found := snapshot.State.Invitations[teamInviteDigest(token)]
	if !found || !validTeamInvitation(invitation, time.Now().UTC()) {
		return "", teamSnapshot{}, teamInvitation{}, errTeamInvitationDenied
	}
	return pointer.Tenant, snapshot, invitation, nil
}

func validTeamInvitation(invitation teamInvitation, now time.Time) bool {
	created, err := time.Parse(time.RFC3339Nano, invitation.CreatedAt)
	if err != nil || now.Before(created) {
		return false
	}
	expires, err := time.Parse(time.RFC3339Nano, invitation.ExpiresAt)
	return err == nil && expires.After(created) && expires.Sub(created) <= teamInviteLifetime && now.Before(expires) &&
		(invitation.Role == roleAdmin || invitation.Role == roleMember) && invitation.Email != ""
}

// The caller must supply an identity whose email was verified by its provider.
// Admission and receipt consumption commit in one team revision. A retry of a
// consumed invite may resume only while the exact subject remains a member.
func (cp *ControlPlane) acceptTeamInvitation(ctx context.Context, token, subject, email, name string) (tenantID string, admitted bool, err error) {
	if !workspaceSubjectPattern.MatchString(subject) {
		return "", false, errTeamForbidden
	}
	for range 4 {
		tenant, snapshot, invitation, err := cp.lookupTeamInvitation(ctx, token)
		if err != nil {
			return "", false, err
		}
		if !strings.EqualFold(invitation.Email, strings.TrimSpace(email)) {
			return "", false, errTeamInvitationDenied
		}
		if invitation.AcceptedBy != "" {
			if invitation.AcceptedBy != subject || teamRole(snapshot.State, subject) == "" {
				return "", false, errTeamInvitationDenied
			}
			return tenant, false, cp.selectMemberWorkspace(ctx, subject, tenant)
		}
		if teamRole(snapshot.State, invitation.InvitedBy) != roleAdmin {
			return "", false, errTeamInvitationDenied
		}
		// Existing members keep their current role, including any intervening
		// demotion. Invitations cannot silently elevate an existing identity.
		if teamRole(snapshot.State, subject) == "" {
			snapshot.State.Members = append(snapshot.State.Members, TeamMember{
				UserID: subject, Email: email, Name: name, Role: invitation.Role,
				JoinedAt: time.Now().UTC().Format(time.RFC3339Nano),
			})
		}
		invitation.AcceptedBy = subject
		snapshot.State.Invitations[teamInviteDigest(token)] = invitation
		if err := cp.saveTeamSnapshot(ctx, tenant, subject, snapshot); err != nil {
			if errors.Is(err, errTeamConflict) {
				continue
			}
			return "", false, err
		}
		return tenant, true, cp.selectMemberWorkspace(ctx, subject, tenant)
	}
	return "", false, errTeamConflict
}

func memberWorkspaceKey(subject string) string {
	return "customer_agent_v1.member_workspace." + teamInviteDigest(subject)
}

// Selecting a workspace is an explicit consequence of accepting its invite.
// The pointer never confers membership; every later login checks the team again.
func (cp *ControlPlane) selectMemberWorkspace(ctx context.Context, subject, tenant string) error {
	response, err := cp.workspaceRequest(ctx).SetBody(map[string]any{
		"key": memberWorkspaceKey(subject), "changed_by": subject,
		"value": map[string]any{"version": 1, "subject_id": subject, "tenant_id": tenant},
	}).Post("/api/v1/config")
	if err != nil || response.StatusCode() != 200 {
		return errTeamUnavailable
	}
	selected, found, err := cp.memberWorkspace(ctx, subject)
	if err != nil || !found || selected != tenant {
		return errTeamUnavailable
	}
	return nil
}

func (cp *ControlPlane) memberWorkspace(ctx context.Context, subject string) (tenantID string, found bool, err error) {
	value, found, err := cp.workspaceConfig(ctx, memberWorkspaceKey(subject))
	if err != nil || !found {
		return "", found, err
	}
	var pointer struct {
		Version int    `json:"version"`
		Subject string `json:"subject_id"`
		Tenant  string `json:"tenant_id"`
	}
	if json.Unmarshal(value, &pointer) != nil || pointer.Version != 1 || pointer.Subject != subject {
		return "", true, errTeamUnavailable
	}
	snapshot, err := cp.readTeamSnapshot(ctx, pointer.Tenant)
	if err != nil {
		return "", true, err
	}
	if teamRole(snapshot.State, subject) == "" {
		return "", true, errWorkspaceMembership
	}
	return pointer.Tenant, true, nil
}
