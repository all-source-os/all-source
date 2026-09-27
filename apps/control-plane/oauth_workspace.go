package main

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"net/http"
	"regexp"
	"time"

	"github.com/go-resty/resty/v2"

	"github.com/allsource/control-plane/internal/application/usecases"
	"github.com/allsource/control-plane/internal/domain/entities"
)

var (
	errWorkspaceUnavailable = errors.New("workspace provisioning unavailable")
	errWorkspaceMembership  = errors.New("workspace membership requires review")
	workspaceSubjectPattern = regexp.MustCompile(`^[A-Za-z0-9_:@.+-]{1,256}$`)
	workspaceIDPattern      = regexp.MustCompile(`^ws-[0-9a-f]{32}$`)
)

// oauthWorkspace runs only after the provider has authenticated the subject.
// The durable, subject-scoped registry is written before creating its random
// tenant, allowing a failed signup to resume without claiming an existing one.
// Legacy email-derived workspaces retain their existing login path; no owner
// membership is inferred or backfilled for them.
func (cp *ControlPlane) oauthWorkspace(ctx context.Context, subject, email, name string) (tenantID string, isNew bool, err error) {
	if !workspaceSubjectPattern.MatchString(subject) || cp.client == nil {
		return "", false, errWorkspaceUnavailable
	}
	if selected, found, err := cp.memberWorkspace(ctx, subject); err != nil || found {
		return selected, false, err
	}
	key := workspaceRegistryKey(subject)
	entry, found, err := cp.workspaceConfig(ctx, key)
	if err != nil {
		return "", false, err
	}
	if !found {
		legacy := entities.TenantSlug(email)
		response, requestErr := cp.workspaceRequest(ctx).Get("/api/v1/tenants/" + legacy)
		if requestErr != nil {
			return "", false, errWorkspaceUnavailable
		}
		if response.StatusCode() == http.StatusOK {
			var tenant struct {
				ID string `json:"id"`
			}
			if json.Unmarshal(response.Body(), &tenant) != nil || tenant.ID != legacy {
				return "", false, errWorkspaceUnavailable
			}
			return legacy, false, nil
		}
		if response.StatusCode() != http.StatusNotFound {
			return "", false, errWorkspaceUnavailable
		}
		suffix := make([]byte, 16)
		if _, err := rand.Read(suffix); err != nil {
			return "", false, errWorkspaceUnavailable
		}
		candidate := map[string]any{
			"version": 1, "subject_id": subject, "tenant_id": "ws-" + hex.EncodeToString(suffix),
		}
		if err := cp.createWorkspaceConfig(ctx, key, candidate, subject); err != nil {
			return "", false, err
		}
		// A concurrent request may have won. Only the durable record chooses the
		// tenant; never continue with this request's losing random candidate.
		entry, found, err = cp.workspaceConfig(ctx, key)
		if err != nil || !found {
			return "", false, errWorkspaceUnavailable
		}
	}
	var registration struct {
		Version int    `json:"version"`
		Subject string `json:"subject_id"`
		Tenant  string `json:"tenant_id"`
	}
	if json.Unmarshal(entry, &registration) != nil || registration.Version != 1 ||
		registration.Subject != subject || !workspaceIDPattern.MatchString(registration.Tenant) {
		return "", false, errWorkspaceUnavailable
	}

	created, err := cp.createRegisteredWorkspace(ctx, registration.Tenant, name)
	if err != nil {
		return "", false, err
	}
	if err := cp.initializeWorkspaceOwner(ctx, registration.Tenant, subject, email, name); err != nil {
		return "", false, err
	}
	return registration.Tenant, created, nil
}

// Email workspaces already use the auth service's immutable user ID, never an
// email-derived slug. Preserve that binding and initialize its persisted owner
// only if no team record exists. Existing membership decisions always win.
func (cp *ControlPlane) emailWorkspace(ctx context.Context, subject, email, name string) (tenantID string, isNew bool, err error) {
	if !authUserIDPattern.MatchString(subject) || cp.client == nil {
		return "", false, errWorkspaceUnavailable
	}
	if selected, found, err := cp.memberWorkspace(ctx, subject); err != nil || found {
		return selected, false, err
	}
	tenantID = "email-" + subject
	created, err := cp.createRegisteredWorkspace(ctx, tenantID, name)
	if err != nil {
		return "", false, err
	}
	if err := cp.initializeWorkspaceOwner(ctx, tenantID, subject, email, name); err != nil {
		return "", false, err
	}
	return tenantID, created, nil
}

func (cp *ControlPlane) initializeWorkspaceOwner(ctx context.Context, tenant, subject, email, name string) error {
	owner := TeamMember{UserID: subject, Email: email, Name: name, Role: roleAdmin, JoinedAt: time.Now().UTC().Format(time.RFC3339)}
	memberKey := teamMembersConfigKey(tenant)
	if err := cp.createWorkspaceConfig(ctx, memberKey, []TeamMember{owner}, subject); err != nil {
		return err
	}
	value, found, err := cp.workspaceConfig(ctx, memberKey)
	if err != nil || !found {
		return errWorkspaceUnavailable
	}
	state, err := decodeTeamState(value)
	if err != nil {
		return errWorkspaceUnavailable
	}
	matched := 0
	for _, member := range state.Members {
		if member.UserID == subject {
			if member.Role != roleAdmin && member.Role != roleMember {
				return errWorkspaceMembership
			}
			matched++
		}
	}
	if matched != 1 {
		return errWorkspaceMembership
	}
	return nil
}

func workspaceRegistryKey(subject string) string {
	hash := sha256.Sum256([]byte(subject))
	return "customer_agent_v1.workspace." + hex.EncodeToString(hash[:])
}

// Request contexts bound all Core calls, including retries configured on the
// existing service client. Responses are bounded and never returned in errors.
func (cp *ControlPlane) workspaceRequest(ctx context.Context) *resty.Request {
	return cp.client.R().SetContext(ctx).SetResponseBodyLimit(65_536)
}

func (cp *ControlPlane) workspaceConfig(ctx context.Context, key string) (json.RawMessage, bool, error) {
	response, err := cp.workspaceRequest(ctx).Get("/api/v1/config/" + key)
	if err != nil {
		return nil, false, errWorkspaceUnavailable
	}
	if response.StatusCode() == http.StatusNotFound {
		return nil, false, nil
	}
	var entry struct {
		Key   string          `json:"key"`
		Value json.RawMessage `json:"value"`
	}
	if response.StatusCode() != http.StatusOK || json.Unmarshal(response.Body(), &entry) != nil || entry.Key != key || len(entry.Value) == 0 {
		return nil, false, errWorkspaceUnavailable
	}
	return entry.Value, true, nil
}

func (cp *ControlPlane) createWorkspaceConfig(ctx context.Context, key string, value any, subject string) error {
	response, err := cp.workspaceRequest(ctx).SetBody(map[string]any{
		"key": key, "value": value, "changed_by": subject, "condition": map[string]string{"kind": "absent"},
	}).Post("/api/v1/config/conditional/set")
	if err != nil {
		return errWorkspaceUnavailable
	}
	// Only a failed precondition is an existing-record candidate. A follower or
	// other conflict must not let stale replicated membership authorize a login.
	if response.StatusCode() == http.StatusConflict {
		var conflict struct {
			Error string `json:"error"`
		}
		if json.Unmarshal(response.Body(), &conflict) != nil || conflict.Error != "Concurrency error: Configuration precondition failed" {
			return errWorkspaceUnavailable
		}
		return nil
	}
	var ack struct {
		Key      string `json:"key"`
		Saved    bool   `json:"saved"`
		Revision string `json:"revision"`
	}
	if response.StatusCode() != http.StatusOK || json.Unmarshal(response.Body(), &ack) != nil || ack.Key != key || !ack.Saved || ack.Revision == "" {
		return errWorkspaceUnavailable
	}
	return nil
}

func (cp *ControlPlane) createRegisteredWorkspace(ctx context.Context, id, name string) (bool, error) {
	subscription, _ := usecases.TrialSubscriptionMetadata(time.Now())
	quota := usecases.TrialQuotaMetadata()
	quota["events_used"] = 0
	quota["queries_used"] = 0
	response, err := cp.workspaceRequest(ctx).SetBody(map[string]any{
		"id": id, "name": name, "quota_preset": "trial",
		"metadata": map[string]any{"subscription": subscription, "quota": quota, "quotas": quota},
	}).Post("/api/v1/tenants")
	if err != nil {
		return false, errWorkspaceUnavailable
	}
	created := response.StatusCode() == http.StatusCreated
	if !created && (response.StatusCode() != http.StatusConflict || string(response.Body()) != "Tenant already exists: "+id) {
		return false, errWorkspaceUnavailable
	}
	// Re-read even after creation: the registry is not proof that Core accepted
	// the exact tenant, and no JWT is issued on a failed or mismatched response.
	response, err = cp.workspaceRequest(ctx).Get("/api/v1/tenants/" + id)
	var tenant struct {
		ID       string                     `json:"id"`
		Metadata map[string]json.RawMessage `json:"metadata"`
	}
	if err != nil || response.StatusCode() != http.StatusOK || json.Unmarshal(response.Body(), &tenant) != nil || tenant.ID != id {
		return false, errWorkspaceUnavailable
	}
	// Missing metadata indicates an incompatible Core or partial legacy state.
	// Never repair it with an unconditional PUT that could overwrite billing.
	var subscriptionState, quotaState map[string]json.RawMessage
	if json.Unmarshal(tenant.Metadata["subscription"], &subscriptionState) != nil || len(subscriptionState) == 0 ||
		json.Unmarshal(tenant.Metadata["quotas"], &quotaState) != nil || len(quotaState) == 0 {
		return false, errWorkspaceUnavailable
	}
	return created, nil
}
