package main

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"regexp"

	"github.com/google/uuid"
)

var (
	errTeamForbidden        = errors.New("current team administrator required")
	errTeamUnavailable      = errors.New("team state unavailable")
	errTeamConflict         = errors.New("team changed; retry the operation")
	errTeamNotFound         = errors.New("team member not found")
	errTeamLastAdmin        = errors.New("team must retain an administrator")
	errTeamInvitationDenied = errors.New("invitation unavailable or does not match your verified email")
	teamTenantPattern       = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$`)
)

// Member changes and invitation receipts share one conditional Core record.
// Keeping admission receipts beside members lets acceptance and removal order
// against the same revision instead of resurrecting a removed user on retry.
type teamState struct {
	SchemaVersion int                       `json:"schema_version"`
	Members       []TeamMember              `json:"members"`
	Invitations   map[string]teamInvitation `json:"invitations"`
}

type teamInvitation struct {
	Email      string `json:"email"`
	Role       string `json:"role"`
	InvitedBy  string `json:"invited_by"`
	CreatedAt  string `json:"created_at"`
	ExpiresAt  string `json:"expires_at"`
	AcceptedBy string `json:"accepted_by,omitempty"`
}

type teamSnapshot struct {
	State    teamState
	Revision string
}

func decodeTeamState(raw json.RawMessage) (teamState, error) {
	state := teamState{SchemaVersion: 2, Invitations: map[string]teamInvitation{}}
	var members []TeamMember
	if json.Unmarshal(raw, &members) == nil && members != nil {
		state.Members = members
	} else {
		state = teamState{}
		if json.Unmarshal(raw, &state) != nil || state.SchemaVersion != 2 || state.Members == nil || state.Invitations == nil {
			return teamState{}, errTeamUnavailable
		}
	}
	if len(state.Members) > 1000 || len(state.Invitations) > 128 {
		return teamState{}, errTeamUnavailable
	}
	seen := make(map[string]bool, len(state.Members))
	for _, member := range state.Members {
		if !workspaceSubjectPattern.MatchString(member.UserID) || seen[member.UserID] ||
			(member.Role != roleAdmin && member.Role != roleMember) {
			return teamState{}, errTeamUnavailable
		}
		seen[member.UserID] = true
	}
	return state, nil
}

func (cp *ControlPlane) readTeamSnapshot(ctx context.Context, tenant string) (teamSnapshot, error) {
	if cp.client == nil || !teamTenantPattern.MatchString(tenant) {
		return teamSnapshot{}, errTeamUnavailable
	}
	key := teamMembersConfigKey(tenant)
	response, err := cp.workspaceRequest(ctx).Get("/api/v1/config/" + key)
	if err != nil || response.StatusCode() != http.StatusOK {
		return teamSnapshot{}, errTeamUnavailable
	}
	var entry struct {
		Key      string          `json:"key"`
		Value    json.RawMessage `json:"value"`
		Revision string          `json:"revision"`
	}
	if json.Unmarshal(response.Body(), &entry) != nil || entry.Key != key {
		return teamSnapshot{}, errTeamUnavailable
	}
	if _, err := uuid.Parse(entry.Revision); err != nil {
		return teamSnapshot{}, errTeamUnavailable
	}
	state, err := decodeTeamState(entry.Value)
	if err != nil {
		return teamSnapshot{}, err
	}
	return teamSnapshot{State: state, Revision: entry.Revision}, nil
}

func teamRole(state teamState, subject string) string {
	for _, member := range state.Members {
		if member.UserID == subject {
			return member.Role
		}
	}
	return ""
}

func (cp *ControlPlane) saveTeamSnapshot(ctx context.Context, tenant, actor string, snapshot teamSnapshot) error {
	encoded, err := json.Marshal(snapshot.State)
	if err != nil || len(encoded) > 60_000 {
		return errTeamUnavailable
	}
	if _, err := decodeTeamState(encoded); err != nil {
		return err
	}
	key := teamMembersConfigKey(tenant)
	response, err := cp.workspaceRequest(ctx).SetBody(map[string]any{
		"key": key, "value": snapshot.State, "changed_by": actor,
		"condition": map[string]string{"kind": "revision", "revision": snapshot.Revision},
	}).Post("/api/v1/config/conditional/set")
	if err != nil {
		return errTeamUnavailable
	}
	var ack struct {
		Key      string `json:"key"`
		Saved    bool   `json:"saved"`
		Revision string `json:"revision"`
		Error    string `json:"error"`
	}
	if json.Unmarshal(response.Body(), &ack) != nil {
		return errTeamUnavailable
	}
	if response.StatusCode() == http.StatusConflict && ack.Error == "Concurrency error: Configuration precondition failed" {
		return errTeamConflict
	}
	if response.StatusCode() != http.StatusOK || ack.Key != key || !ack.Saved {
		return errTeamUnavailable
	}
	if _, err := uuid.Parse(ack.Revision); err != nil || ack.Revision == snapshot.Revision {
		return errTeamUnavailable
	}
	return nil
}

func (cp *ControlPlane) editTeam(ctx context.Context, actor *AuthContext, edit func(*teamState) error) error {
	if actor == nil || actor.IsAPIKey || actor.ViewAs || !workspaceSubjectPattern.MatchString(actor.UserID) {
		return errTeamForbidden
	}
	for range 4 {
		snapshot, err := cp.readTeamSnapshot(ctx, actor.TenantID)
		if err != nil {
			return err
		}
		if teamRole(snapshot.State, actor.UserID) != roleAdmin {
			return errTeamForbidden
		}
		if err := edit(&snapshot.State); err != nil {
			return err
		}
		err = cp.saveTeamSnapshot(ctx, actor.TenantID, actor.UserID, snapshot)
		if !errors.Is(err, errTeamConflict) {
			return err
		}
	}
	return errTeamConflict
}

func removeTeamMember(state *teamState, subject string) error {
	for i, member := range state.Members {
		if member.UserID == subject {
			if member.Role == roleAdmin && countTeamAdmins(*state) < 2 {
				return errTeamLastAdmin
			}
			state.Members = append(state.Members[:i], state.Members[i+1:]...)
			return nil
		}
	}
	return errTeamNotFound
}

func changeTeamRole(state *teamState, subject, role string) error {
	if role != roleAdmin && role != roleMember {
		return errTeamForbidden
	}
	for i, member := range state.Members {
		if member.UserID == subject {
			if member.Role == roleAdmin && role != roleAdmin && countTeamAdmins(*state) < 2 {
				return errTeamLastAdmin
			}
			state.Members[i].Role = role
			return nil
		}
	}
	return errTeamNotFound
}

func countTeamAdmins(state teamState) int {
	count := 0
	for _, member := range state.Members {
		if member.Role == roleAdmin {
			count++
		}
	}
	return count
}
