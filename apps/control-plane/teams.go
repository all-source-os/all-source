package main

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// AgentKeyMeta is the persisted metadata for a provisioned agent API key.
// The actual key value is returned once at creation and never stored.
type AgentKeyMeta struct {
	Name      string `json:"name"`
	KeyID     string `json:"key_id"`
	CreatedAt string `json:"created_at"`
}

// TeamMember represents a member of a team (tenant).
type TeamMember struct {
	UserID   string `json:"user_id"`
	Email    string `json:"email"`
	Name     string `json:"name"`
	Role     string `json:"role"`
	JoinedAt string `json:"joined_at"`
}

// InviteRequest is the request body for creating an invite.
type InviteRequest struct {
	Email string `json:"email" binding:"required"`
	Role  string `json:"role"` // "admin" or "member" (default: "member")
}

const (
	roleAdmin  = "admin"
	roleMember = "member"
)

// teamMembersConfigKey returns the Core config key for a tenant's member list.
func teamMembersConfigKey(tenantID string) string {
	return "team:" + tenantID + ":members"
}

// generateInviteToken generates a random URL-safe token.
func generateInviteToken() (string, error) {
	b := make([]byte, 24)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(b), nil
}

// InviteHandler creates a team invite.
// POST /api/v1/teams/invite
// Requires: admin role
func (cp *ControlPlane) InviteHandler(c *gin.Context) {
	authCtx := mustAuthContext(c)
	if authCtx == nil {
		return
	}
	var req InviteRequest
	c.Request.Body = http.MaxBytesReader(c.Writer, c.Request.Body, 4096)
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(400, gin.H{"error": "invalid request", "message": "Enter a valid email and team role."})
		return
	}

	role := req.Role
	if role == "" {
		role = roleMember
	}
	if role != roleAdmin && role != roleMember {
		c.JSON(400, gin.H{"error": "invalid role", "message": "role must be 'admin' or 'member'"})
		return
	}

	ctx, cancel := context.WithTimeout(c.Request.Context(), 30*time.Second)
	defer cancel()
	token, invite, err := cp.createTeamInvitation(ctx, authCtx, req.Email, role)
	if err != nil {
		respondTeamError(c, err)
		return
	}
	c.Header("Cache-Control", "no-store")
	c.JSON(201, gin.H{
		"token":      token,
		"email":      invite.Email,
		"role":       role,
		"created_at": invite.CreatedAt,
		"expires_at": invite.ExpiresAt,
	})
}

// GetInviteHandler returns invite details for the given token.
// GET /api/v1/teams/invite/:token
// Public — used by the frontend to show the user what they're joining before OAuth.
func (cp *ControlPlane) GetInviteHandler(c *gin.Context) {
	c.Header("Cache-Control", "no-store")
	token := c.Param("token")
	if token == "" {
		c.JSON(400, gin.H{"error": "missing token"})
		return
	}

	ctx, cancel := context.WithTimeout(c.Request.Context(), 30*time.Second)
	defer cancel()
	tenant, _, invite, err := cp.lookupTeamInvitation(ctx, token)
	if err != nil {
		c.JSON(404, gin.H{"error": "not_found", "message": "invite not found or expired"})
		return
	}

	if invite.AcceptedBy != "" {
		c.JSON(410, gin.H{"error": "invite already accepted"})
		return
	}
	c.Header("Cache-Control", "no-store")
	c.JSON(200, gin.H{
		"email":      invite.Email,
		"role":       invite.Role,
		"tenant_id":  tenant,
		"created_at": invite.CreatedAt,
		"expires_at": invite.ExpiresAt,
	})
}

// ListMembersHandler lists all members of the caller's team.
// GET /api/v1/teams/members
func (cp *ControlPlane) ListMembersHandler(c *gin.Context) {
	authCtx := mustAuthContext(c)
	if authCtx == nil {
		return
	}

	ctx, cancel := context.WithTimeout(c.Request.Context(), 30*time.Second)
	defer cancel()
	snapshot, err := cp.readTeamSnapshot(ctx, authCtx.TenantID)
	if err != nil {
		respondTeamError(c, err)
		return
	}
	role := teamRole(snapshot.State, authCtx.UserID)
	if authCtx.IsAPIKey || role == "" {
		respondTeamError(c, errTeamForbidden)
		return
	}
	c.Header("Cache-Control", "no-store")
	c.JSON(200, gin.H{
		"members": snapshot.State.Members, "seats_used": len(snapshot.State.Members),
		"seat_limit": nil, "can_manage": role == roleAdmin, "current_user_id": authCtx.UserID,
	})
}

// DeleteMemberHandler removes a member from the team.
// DELETE /api/v1/teams/members/:id
// Requires: admin role
func (cp *ControlPlane) DeleteMemberHandler(c *gin.Context) {
	authCtx := mustAuthContext(c)
	if authCtx == nil {
		return
	}
	memberID := c.Param("id")
	if memberID == "" {
		c.JSON(400, gin.H{"error": "missing member id"})
		return
	}
	if memberID == authCtx.UserID {
		c.JSON(400, gin.H{"error": "invalid_operation", "message": "cannot remove yourself from the team"})
		return
	}

	ctx, cancel := context.WithTimeout(c.Request.Context(), 30*time.Second)
	defer cancel()
	if err := cp.editTeam(ctx, authCtx, func(state *teamState) error {
		return removeTeamMember(state, memberID)
	}); err != nil {
		respondTeamError(c, err)
		return
	}

	c.Status(204)
}

// UpdateMemberRoleHandler changes the role of a team member.
// PUT /api/v1/teams/members/:id
// Requires: admin role
func (cp *ControlPlane) UpdateMemberRoleHandler(c *gin.Context) {
	authCtx := mustAuthContext(c)
	if authCtx == nil {
		return
	}
	memberID := c.Param("id")
	if memberID == "" {
		c.JSON(400, gin.H{"error": "missing member id"})
		return
	}

	var req struct {
		Role string `json:"role" binding:"required"`
	}
	c.Request.Body = http.MaxBytesReader(c.Writer, c.Request.Body, 4096)
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(400, gin.H{"error": "invalid request", "message": "Enter a valid team role."})
		return
	}
	if req.Role != roleAdmin && req.Role != "member" {
		c.JSON(400, gin.H{"error": "invalid role", "message": "role must be 'admin' or 'member'"})
		return
	}

	ctx, cancel := context.WithTimeout(c.Request.Context(), 30*time.Second)
	defer cancel()
	if err := cp.editTeam(ctx, authCtx, func(state *teamState) error {
		return changeTeamRole(state, memberID, req.Role)
	}); err != nil {
		respondTeamError(c, err)
		return
	}

	c.JSON(200, gin.H{"message": "role updated"})
}

func respondTeamError(c *gin.Context, err error) {
	status, code := 503, "team_unavailable"
	switch {
	case errors.Is(err, errTeamForbidden), errors.Is(err, errTeamInvitationDenied):
		status, code = 403, "forbidden"
	case errors.Is(err, errTeamConflict), errors.Is(err, errTeamLastAdmin):
		status, code = 409, "team_conflict"
	case errors.Is(err, errTeamNotFound):
		status, code = 404, "not_found"
	}
	c.Header("Cache-Control", "no-store")
	c.JSON(status, gin.H{"error": gin.H{"code": code, "message": err.Error()}})
}

// teamAgentKeysConfigKey returns the Core config key for a tenant's agent key list.
func teamAgentKeysConfigKey(tenantID string) string {
	return "team:" + tenantID + ":agent_keys"
}

// CreateAgentKeyHandler provisions a new Core API key for an agent working in the team's tenant.
// POST /api/v1/teams/agent-keys
// The raw key value is returned exactly once — it is never stored or retrievable again.
func (cp *ControlPlane) CreateAgentKeyHandler(c *gin.Context) {
	authCtx := mustAuthContext(c)
	if authCtx == nil {
		return
	}

	var req struct {
		Name string `json:"name" binding:"required"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(400, gin.H{"error": "invalid_request", "message": err.Error()})
		return
	}

	// Create a ServiceAccount key scoped to the team tenant in Core.
	resp, err := cp.coreClient.CreateCoreAPIKey(c.Request.Context(), clients.CreateCoreAPIKeyRequest{
		Name:     req.Name,
		TenantID: authCtx.TenantID,
		Role:     "serviceaccount",
	})
	if err != nil {
		c.JSON(500, gin.H{"error": "internal_error", "message": "failed to create agent key"})
		return
	}

	// Persist metadata (name + Core key ID) so the key can be listed and revoked later.
	// The raw key value is NOT stored — it is only returned in this response.
	createdAt := time.Now().UTC().Format(time.RFC3339)
	meta := AgentKeyMeta{Name: req.Name, KeyID: resp.ID, CreatedAt: createdAt}

	existing, _ := cp.getAgentKeyMetas(c.Request.Context(), authCtx.TenantID) //nolint:errcheck
	existing = append(existing, meta)
	if saveErr := cp.saveAgentKeyMetas(c.Request.Context(), authCtx.TenantID, existing, authCtx.UserID); saveErr != nil {
		// Non-fatal: the key was created in Core; warn but still return it.
		c.Header("X-Warning", "key created but metadata save failed; note the key now")
	}

	c.JSON(201, gin.H{
		"name":       req.Name,
		"key":        resp.Key,
		"tenant_id":  authCtx.TenantID,
		"created_at": createdAt,
	})
}

// ListAgentKeysHandler returns metadata for all agent keys provisioned for the team.
// GET /api/v1/teams/agent-keys
// Key values are never returned — only name, key_id, and created_at.
func (cp *ControlPlane) ListAgentKeysHandler(c *gin.Context) {
	authCtx := mustAuthContext(c)
	if authCtx == nil {
		return
	}

	metas, err := cp.getAgentKeyMetas(c.Request.Context(), authCtx.TenantID)
	if err != nil {
		metas = []AgentKeyMeta{}
	}

	c.JSON(200, gin.H{"agent_keys": metas})
}

// RevokeAgentKeyHandler revokes an agent key by name, removing it from Core and the metadata list.
// DELETE /api/v1/teams/agent-keys/:name
func (cp *ControlPlane) RevokeAgentKeyHandler(c *gin.Context) {
	authCtx := mustAuthContext(c)
	if authCtx == nil {
		return
	}

	name := c.Param("name")
	if name == "" {
		c.JSON(400, gin.H{"error": "missing key name"})
		return
	}

	metas, err := cp.getAgentKeyMetas(c.Request.Context(), authCtx.TenantID)
	if err != nil {
		c.JSON(404, gin.H{"error": "not_found", "message": "no agent keys found"})
		return
	}

	var keyID string
	updated := make([]AgentKeyMeta, 0, len(metas))
	for _, m := range metas {
		if m.Name == name {
			keyID = m.KeyID
			continue
		}
		updated = append(updated, m)
	}

	if keyID == "" {
		c.JSON(404, gin.H{"error": "not_found", "message": "agent key not found"})
		return
	}

	// Revoke in Core — this makes the key immediately invalid.
	if revokeErr := cp.coreClient.RevokeAPIKey(c.Request.Context(), keyID); revokeErr != nil {
		c.JSON(500, gin.H{"error": "internal_error", "message": "failed to revoke key in Core"})
		return
	}

	// Remove from metadata list.
	if saveErr := cp.saveAgentKeyMetas(c.Request.Context(), authCtx.TenantID, updated, authCtx.UserID); saveErr != nil {
		// Key is already revoked in Core; metadata cleanup failure is non-fatal.
		c.Header("X-Warning", "key revoked but metadata cleanup failed")
	}

	c.Status(204)
}

// getAgentKeyMetas fetches the agent key metadata list from Core config.
func (cp *ControlPlane) getAgentKeyMetas(ctx context.Context, tenantID string) ([]AgentKeyMeta, error) {
	entry, err := cp.coreClient.GetConfig(ctx, teamAgentKeysConfigKey(tenantID))
	if err != nil || entry == nil {
		return nil, fmt.Errorf("no agent keys stored")
	}
	return parseAgentKeyMetasFromConfig(entry.Value)
}

// saveAgentKeyMetas persists the agent key metadata list to Core config.
func (cp *ControlPlane) saveAgentKeyMetas(ctx context.Context, tenantID string, metas []AgentKeyMeta, callerID string) error {
	return cp.coreClient.SetConfig(ctx, clients.SetConfigRequest{
		Key:       teamAgentKeysConfigKey(tenantID),
		Value:     metas,
		ChangedBy: callerID,
	})
}

// parseAgentKeyMetasFromConfig converts a raw config value into []AgentKeyMeta.
func parseAgentKeyMetasFromConfig(raw any) ([]AgentKeyMeta, error) {
	b, err := json.Marshal(raw)
	if err != nil {
		return nil, fmt.Errorf("marshal agent key metas: %w", err)
	}
	var metas []AgentKeyMeta
	if err := json.Unmarshal(b, &metas); err != nil {
		return nil, fmt.Errorf("unmarshal agent key metas: %w", err)
	}
	return metas, nil
}

// mustAuthContext extracts the auth context from gin or responds 401 and returns nil.
func mustAuthContext(c *gin.Context) *AuthContext {
	authCtx, exists := c.Get("auth")
	if !exists {
		c.JSON(401, gin.H{"error": "unauthorized"})
		return nil
	}
	ac, ok := authCtx.(*AuthContext)
	if !ok {
		c.JSON(401, gin.H{"error": "unauthorized"})
		return nil
	}
	return ac
}
