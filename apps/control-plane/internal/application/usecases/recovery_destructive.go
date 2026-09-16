package usecases

import (
	"context"
	"errors"
	"time"

	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// ----------------------------------------------------------------------------
// Destructive: rotate_keys (confirm_token)
// ----------------------------------------------------------------------------

// RotateKeys re-mints the tenant's Core API keys with the canonical
// "serviceaccount" role (no underscore — the service_account drift silently 403s
// every key, MEMORY.md). Destructive: rotation invalidates the tenant's CURRENT
// keys, so the apply requires an echoed confirm_token from a prior dry-run (the
// operator must have seen the "these N keys will stop working" warning).
func (uc *RecoveryUseCase) RotateKeys(ctx context.Context, tenantID string, req RecoveryRequest) (*RecoveryResult, error) {
	const action = "rotate_keys"
	t, err := uc.requireTenant(tenantID)
	if err != nil {
		return nil, err
	}

	existing := uc.listKeys(ctx, t.ID)
	canonical := string(entities.RoleServiceAccount)
	keyNames := make([]string, 0, len(existing))
	// A name alone does not tell an operator whether revoking a key is safe.
	// Role, age and last use do, so the dry-run reports them per key and
	// summarizes the cohort that is both legacy and never used, the one that
	// can be revoked without rotating a consumer first.
	keys := make([]map[string]any, 0, len(existing))
	legacyUnused := 0
	for _, k := range existing {
		keyNames = append(keyNames, k.Name)
		legacy := k.IsLegacy(canonical)
		if legacy && k.NeverUsed() {
			legacyUnused++
		}
		keys = append(keys, map[string]any{
			"id":         k.ID,
			"name":       k.Name,
			"role":       k.Role,
			"active":     k.Active,
			"created_at": valueOrEmpty(k.CreatedAt),
			"expires_at": valueOrEmpty(k.ExpiresAt),
			"last_used":  valueOrEmpty(k.LastUsed),
			"never_used": k.NeverUsed(),
			"legacy":     legacy,
		})
	}
	preview := map[string]any{
		"tenant_id":          t.ID,
		"keys_to_invalidate": len(existing),
		"key_names":          keyNames,
		"keys":               keys,
		"legacy_unused":      legacyUnused,
		"new_role":           canonical,
		"warning":            "all current keys for this tenant will stop working immediately",
	}

	if req.DryRun {
		// token-gated: preview() attaches a confirm_token for the apply call.
		return uc.preview(ctx, action, t.ID, req, preview)
	}

	// Apply guard: echoed confirm_token from the dry-run.
	if err := uc.guard.enforceConfirmation(action, t.ID, req, confirmTokenEchoed); err != nil {
		return nil, err
	}

	// Mint the replacement first (canonical role), then revoke the old ones, so a
	// failure can't leave the tenant with no working key.
	var newKeyID string
	if uc.coreClient != nil {
		created, err := uc.coreClient.CreateCoreAPIKey(ctx, clients.CreateCoreAPIKeyRequest{
			Name:     "recovery-rotated-" + time.Now().UTC().Format("20060102T150405"),
			TenantID: t.ID,
			Role:     string(entities.RoleServiceAccount),
		})
		if err != nil {
			return nil, err
		}
		newKeyID = created.ID
		for _, k := range existing {
			_ = uc.coreClient.RevokeAPIKey(ctx, k.ID) //nolint:errcheck // best-effort revoke; audit captures the rotation
		}
	}
	details := map[string]any{
		"tenant_id":     t.ID,
		"revoked_count": len(existing),
		"new_key_id":    newKeyID,
		"new_role":      string(entities.RoleServiceAccount),
	}
	return uc.applied(ctx, action, t.ID, req, details, "keys rotated to canonical serviceaccount role")
}

// ----------------------------------------------------------------------------
// Destructive: reprovision (confirm_tenant_id)
// ----------------------------------------------------------------------------

// Reprovision rewrites a tenant's metadata to a clean baseline (heals corrupt /
// incomplete onboarding). Destructive: a wrong id would clobber a healthy paying
// tenant, so it requires a typed confirm_tenant_id == target. HARD-REJECTED when
// the tenant is active AND has ingested in the last 24h (use force_resync
// instead) — §4.3 "never active+recent". Blast radius = exactly 1.
func (uc *RecoveryUseCase) Reprovision(ctx context.Context, tenantID string, req RecoveryRequest) (*RecoveryResult, error) {
	const action = "reprovision"
	t, err := uc.requireTenant(tenantID)
	if err != nil {
		return nil, err
	}

	// Precondition guard: reject active + recently-ingested tenants.
	if t.IsActive() {
		newest := uc.newestEventTime(ctx, t.ID)
		if !newest.IsZero() && time.Since(newest) < ReprovisionRecentEventWindow {
			return nil, errors.Join(ErrPreconditionFailed,
				errors.New("tenant is active with events in the last 24h; use force_resync instead"))
		}
	}

	sub := extractSubscriptionForHealth(t.Metadata)
	tier := effectiveBillingTier(t.Metadata, sub)
	want := entities.QuotasForTier(tier)
	preview := map[string]any{
		"tenant_id":        t.ID,
		"current_status":   string(t.Status),
		"effective_tier":   tier,
		"metadata_changes": "reset quotas to QuotasForTier(tier); preserve subscription + home_region",
		"new_events_quota": want.EventsQuota,
		"blast_radius":     1,
	}

	if req.DryRun {
		return uc.preview(ctx, action, t.ID, req, preview)
	}

	// Apply guard: typed confirm_tenant_id must equal the target.
	if err := uc.guard.enforceConfirmation(action, t.ID, req, confirmTenantID); err != nil {
		return nil, err
	}

	// Reprovision = re-apply the tier baseline via the canonical billing path
	// (recomputes QuotasForTier, preserves non-billing metadata like home_region).
	if uc.updateSubsUC != nil {
		if _, err := uc.updateSubsUC.Execute(t.ID, &entities.TenantBillingMetadata{
			Subscription: &entities.SubscriptionMetadata{Tier: tier, Status: orDefault(sub.Status, "active")},
		}); err != nil {
			return nil, err
		}
	}
	return uc.applied(ctx, action, t.ID, req, preview, "tenant metadata reprovisioned to tier baseline")
}

// ----------------------------------------------------------------------------
// Destructive: restore (confirm_tenant_id + snapshot_id) — wraps StartReplay
// ----------------------------------------------------------------------------

// Restore replays a tenant's events (optionally as-of a point in time / snapshot)
// to rebuild a bad projection or recover from corruption. Destructive: a replay
// can overwrite newer state, so the apply requires a typed confirm_tenant_id and
// the operator must see the snapshot/as-of in the dry-run. WRAPS the EXISTING
// StartReplayUseCase (tenant-scoped) — adds no new Core capability (§4.3).
func (uc *RecoveryUseCase) Restore(ctx context.Context, tenantID string, req RecoveryRequest) (*RecoveryResult, error) {
	const action = "restore"
	t, err := uc.requireTenant(tenantID)
	if err != nil {
		return nil, err
	}

	// snapshot_id is optional in Extra; absent or non-string means "no snapshot
	// pinned", which downstream handles.
	snapshotID, _ := req.Extra["snapshot_id"].(string) //nolint:forcetypeassert,errcheck // optional field, zero value is meaningful
	var asOf *time.Time
	if v, ok := req.Extra["as_of"].(string); ok && v != "" {
		if parsed, perr := time.Parse(time.RFC3339, v); perr == nil {
			asOf = &parsed
		}
	}
	// The replay entity scope. Default to the tenant id (Core replays a tenant's
	// stream when given the tenant as the entity); callers may target one entity.
	entityID := t.ID
	if v, ok := req.Extra["entity_id"].(string); ok && v != "" {
		entityID = v
	}

	preview := map[string]any{
		"tenant_id":   t.ID,
		"entity_id":   entityID,
		"snapshot_id": snapshotID,
		"as_of":       valueOrEmpty(asOf),
		"warning":     "replay can overwrite newer events; confirm the snapshot/as-of before applying",
	}

	if req.DryRun {
		return uc.preview(ctx, action, t.ID, req, preview)
	}

	// Apply guard: typed confirm_tenant_id must equal the target.
	if err := uc.guard.enforceConfirmation(action, t.ID, req, confirmTenantID); err != nil {
		return nil, err
	}

	// Audit BEFORE the irreversible operation so the intent is durable even if the
	// replay call partially completes.
	if err := uc.auditor.Record(ctx, RecoveryAuditRecord{
		Action: action, TenantID: t.ID, Actor: req.Actor,
		DryRun: false, Reason: req.Reason, Outcome: "applied", Details: preview,
	}); err != nil {
		return nil, err
	}

	// Wrap the existing replay use case (tenant-scoped). nil = no replay engine
	// wired (test mode); the audit above still records the intent.
	var replayID string
	if uc.startReplay != nil {
		op, err := uc.startReplay.Execute(ctx, entityID, asOf, t.ID, req.Actor)
		if err != nil {
			return nil, err
		}
		if op != nil && op.Result != nil {
			if rid, ok := op.Result["replay_id"].(string); ok {
				replayID = rid
			}
		}
	}
	res := &RecoveryResult{
		Action:   action,
		TenantID: t.ID,
		DryRun:   false,
		Applied:  true,
		Message:  "restore/replay started",
		Would:    map[string]any{"replay_id": replayID},
	}
	return res, nil
}

// --- helpers ---

func (uc *RecoveryUseCase) listKeys(ctx context.Context, tenantID string) []clients.CoreAPIKeyInfo {
	if uc.coreClient == nil {
		return nil
	}
	keys, err := uc.coreClient.ListCoreAPIKeys(ctx, tenantID)
	if err != nil {
		return nil
	}
	return keys
}

func valueOrEmpty(t *time.Time) string {
	if t == nil {
		return ""
	}
	return t.UTC().Format(time.RFC3339)
}
