package usecases

import (
	"context"
	"encoding/json"
	"strings"

	"github.com/allsource/control-plane/internal/application/dto"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/domain/repositories"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// defaultPlan is the READ-TIME fallback tier for a tenant whose metadata
// carries no subscription.tier. It is deliberately still "free" — and ONLY a
// read-time fallback, NEVER a creation default (prompt 048).
//
// A tenant with no tier in metadata genuinely PREDATES the trial rollout (every
// new self-service mint now stamps subscription.tier="trial" + trial_expires_at,
// see usecases.TrialSubscriptionMetadata). Relabeling those legacy tenants as
// "trial" would (a) rewrite history and (b) hand them a 14-day clock they never
// had, suspending grandfathered tenants out of the blue. So a missing tier reads
// as the grandfathered "free", and the operator migrates real free tenants to
// comped enterprise via docs/runbooks/RETIRE_FREE_PLAN.md — never automatically.
//
// The no-free-as-a-CREATION-default invariant is enforced by
// TestNoFreeTierMintedAsCreationDefault; this read-time fallback is exempt
// because it inspects EXISTING tenants, it does not mint new ones.
const defaultPlan = "free"

// ListTenantsRequest holds pagination and filter parameters.
type ListTenantsRequest struct {
	Offset int
	Limit  int
	Status string // optional: "active", "suspended", "deleted"; empty means all
	Search string // optional: case-insensitive substring match on tenant name
	Plan   string // optional: filter by subscription tier (free, pro, team)
}

// ListTenantsResponse wraps the paginated result.
type ListTenantsResponse struct {
	Tenants []*dto.TenantResponse `json:"tenants"`
	Total   int                   `json:"total"`
}

// teamMembersConfigKey returns the Core config key holding a tenant's member
// list. Mirrors the key written by the team-management handlers
// (apps/control-plane/teams.go teamMembersConfigKey); duplicated here because
// that helper lives in package main and cannot be imported from a use case.
func teamMembersConfigKey(tenantID string) string {
	return "team:" + tenantID + ":members"
}

// ListTenantsUseCase handles listing tenants with pagination and status filter.
type ListTenantsUseCase struct {
	tenantRepo repositories.TenantRepository
	// coreClient sources REAL per-tenant counts (events via GET
	// /api/v1/tenants/{id}/stats, members via the team-members config) for the
	// admin enrichment path. It may be nil (e.g. unit tests / Core unavailable),
	// in which case enrichment falls back to the metadata mirror and a guarded 0.
	coreClient clients.CoreClient
}

// NewListTenantsUseCase creates a new ListTenantsUseCase. coreClient may be nil;
// when nil, admin enrichment falls back to the tenant-metadata mirror instead of
// live Core stats (counts then reflect whatever the metering path last wrote).
func NewListTenantsUseCase(tenantRepo repositories.TenantRepository, coreClient clients.CoreClient) *ListTenantsUseCase {
	return &ListTenantsUseCase{tenantRepo: tenantRepo, coreClient: coreClient}
}

// Execute retrieves tenants with optional status filter and pagination.
func (uc *ListTenantsUseCase) Execute(req ListTenantsRequest) (*ListTenantsResponse, error) {
	var tenants []*entities.Tenant
	var err error

	// Filter by status if specified
	switch entities.TenantStatus(req.Status) {
	case entities.TenantStatusActive:
		tenants, err = uc.tenantRepo.FindActive()
	case entities.TenantStatusSuspended, entities.TenantStatusDeleted, entities.TenantStatusArchived:
		tenants, err = uc.tenantRepo.FindAll()
	default:
		tenants, err = uc.tenantRepo.FindAll()
	}
	if err != nil {
		return nil, err
	}

	// Apply in-memory status filter for non-active statuses (suspended, deleted, archived)
	if req.Status != "" && req.Status != string(entities.TenantStatusActive) {
		filtered := make([]*entities.Tenant, 0)
		for _, t := range tenants {
			if string(t.Status) == req.Status {
				filtered = append(filtered, t)
			}
		}
		tenants = filtered
	}

	// Apply search filter (case-insensitive substring match on name)
	if req.Search != "" {
		filtered := make([]*entities.Tenant, 0, len(tenants))
		for _, t := range tenants {
			if strings.Contains(strings.ToLower(t.Name), strings.ToLower(req.Search)) {
				filtered = append(filtered, t)
			}
		}
		tenants = filtered
	}

	// Apply plan filter (match subscription tier in metadata)
	if req.Plan != "" {
		filtered := make([]*entities.Tenant, 0, len(tenants))
		for _, t := range tenants {
			tenantPlan := extractPlan(t)
			if strings.EqualFold(tenantPlan, req.Plan) {
				filtered = append(filtered, t)
			}
		}
		tenants = filtered
	}

	total := len(tenants)

	// Apply pagination
	if req.Offset > 0 {
		if req.Offset >= len(tenants) {
			tenants = nil
		} else {
			tenants = tenants[req.Offset:]
		}
	}
	if req.Limit > 0 && req.Limit < len(tenants) {
		tenants = tenants[:req.Limit]
	}

	// Convert to DTOs
	responses := make([]*dto.TenantResponse, len(tenants))
	for i, t := range tenants {
		responses[i] = &dto.TenantResponse{
			ID:          t.ID,
			Name:        t.Name,
			Description: t.Description,
			Status:      string(t.Status),
			HomeRegion:  t.EffectiveHomeRegion(),
			CreatedAt:   t.CreatedAt,
			UpdatedAt:   t.UpdatedAt,
			Metadata:    t.Metadata,
		}
	}

	return &ListTenantsResponse{
		Tenants: responses,
		Total:   total,
	}, nil
}

// ExecuteAdmin retrieves tenants with admin-level enrichment (plan, event_count, member_count)
// and page-based pagination. Per-tenant counts are sourced from live Core stats
// (events) and the team-members config (members) for the current page only, so a
// large fleet doesn't fan out one stats call per tenant on every list; tenants
// outside the page are never queried.
func (uc *ListTenantsUseCase) ExecuteAdmin(ctx context.Context, req ListTenantsRequest) (*dto.AdminListTenantsResponse, error) {
	var tenants []*entities.Tenant
	var err error

	// Fetch tenants based on status filter
	switch entities.TenantStatus(req.Status) {
	case entities.TenantStatusActive:
		tenants, err = uc.tenantRepo.FindActive()
	case entities.TenantStatusSuspended, entities.TenantStatusDeleted, entities.TenantStatusArchived:
		tenants, err = uc.tenantRepo.FindAll()
	default:
		tenants, err = uc.tenantRepo.FindAll()
	}
	if err != nil {
		return nil, err
	}

	// Apply in-memory status filter for non-active statuses
	if req.Status != "" && req.Status != string(entities.TenantStatusActive) {
		filtered := make([]*entities.Tenant, 0)
		for _, t := range tenants {
			if string(t.Status) == req.Status {
				filtered = append(filtered, t)
			}
		}
		tenants = filtered
	}

	// Apply search filter
	if req.Search != "" {
		filtered := make([]*entities.Tenant, 0, len(tenants))
		for _, t := range tenants {
			if strings.Contains(strings.ToLower(t.Name), strings.ToLower(req.Search)) {
				filtered = append(filtered, t)
			}
		}
		tenants = filtered
	}

	// Apply plan filter
	if req.Plan != "" {
		filtered := make([]*entities.Tenant, 0, len(tenants))
		for _, t := range tenants {
			tenantPlan := extractPlan(t)
			if strings.EqualFold(tenantPlan, req.Plan) {
				filtered = append(filtered, t)
			}
		}
		tenants = filtered
	}

	total := len(tenants)

	// Page-based pagination
	page := 1
	perPage := 20
	if req.Offset > 0 {
		page = req.Offset // reuse Offset as page for admin endpoint
	}
	if req.Limit > 0 {
		perPage = req.Limit // reuse Limit as perPage for admin endpoint
	}

	totalPages := total / perPage
	if total%perPage != 0 {
		totalPages++
	}
	if totalPages == 0 {
		totalPages = 1
	}

	// Calculate slice bounds from page-based pagination
	start := (page - 1) * perPage
	if start >= total {
		tenants = nil
	} else {
		end := start + perPage
		if end > total {
			end = total
		}
		tenants = tenants[start:end]
	}

	// Convert to admin DTOs with enrichment. Counts come from live Core for the
	// current page; on any miss we fall back to the metadata mirror so a count is
	// always a guarded number, never undefined/omitted.
	responses := make([]*dto.AdminTenantResponse, len(tenants))
	for i, t := range tenants {
		responses[i] = &dto.AdminTenantResponse{
			ID:          t.ID,
			Name:        t.Name,
			Plan:        extractPlan(t),
			Status:      string(t.Status),
			CreatedAt:   t.CreatedAt,
			EventCount:  uc.eventCountForTenant(ctx, t),
			MemberCount: uc.memberCountForTenant(ctx, t),
		}
	}

	return &dto.AdminListTenantsResponse{
		Tenants:    responses,
		Total:      total,
		Page:       page,
		PerPage:    perPage,
		TotalPages: totalPages,
	}, nil
}

// extractPlan reads the subscription tier from tenant metadata.
// Returns defaultPlan ("free") if not set.
func extractPlan(t *entities.Tenant) string {
	if t.Metadata == nil {
		return defaultPlan
	}
	sub, ok := t.Metadata["subscription"]
	if !ok {
		return defaultPlan
	}
	subMap, ok := sub.(map[string]interface{})
	if !ok {
		return defaultPlan
	}
	tier, ok := subMap["tier"].(string)
	if !ok || tier == "" {
		return defaultPlan
	}
	return tier
}

// eventCountForTenant returns the tenant's real event total from live Core stats
// (GET /api/v1/tenants/{id}/stats), falling back to the metadata mirror when Core
// is unavailable or the client is nil. Always returns a guarded number.
func (uc *ListTenantsUseCase) eventCountForTenant(ctx context.Context, t *entities.Tenant) int64 {
	if uc.coreClient != nil {
		if stats, err := uc.coreClient.GetTenantStats(ctx, t.ID); err == nil && stats != nil {
			return stats.EventCount
		}
		// Core unreachable → fall through to the metadata mirror rather than fail.
	}
	return extractEventCount(t)
}

// memberCountForTenant returns the tenant's real member count from the Core
// team-members config (the same list the team handlers maintain), falling back to
// the metadata mirror when Core is unavailable or the client is nil.
func (uc *ListTenantsUseCase) memberCountForTenant(ctx context.Context, t *entities.Tenant) int {
	if uc.coreClient != nil {
		if n, ok := memberCountFromCore(ctx, uc.coreClient, t.ID); ok {
			return n
		}
	}
	return extractMemberCount(t)
}

// memberCountFromCore reads team:<id>:members from Core config and returns the
// number of members. ok is false when the client errors so the caller can fall
// back; a present-but-empty / unset member list is a valid 0 (ok=true).
func memberCountFromCore(ctx context.Context, core clients.CoreClient, tenantID string) (int, bool) {
	entry, err := core.GetConfig(ctx, teamMembersConfigKey(tenantID))
	if err != nil {
		return 0, false
	}
	if entry == nil || entry.Value == nil {
		return 0, true // no member list stored yet → a clean 0
	}
	// entry.Value is the JSON array of members; count its elements without
	// importing the team-member struct (which lives in package main).
	b, err := json.Marshal(entry.Value)
	if err != nil {
		return 0, false
	}
	var members []json.RawMessage
	if err := json.Unmarshal(b, &members); err != nil {
		var state struct {
			Version     int                        `json:"schema_version"`
			Members     []json.RawMessage          `json:"members"`
			Invitations map[string]json.RawMessage `json:"invitations"`
		}
		if json.Unmarshal(b, &state) != nil || state.Version != 2 || state.Members == nil || state.Invitations == nil {
			return 0, false
		}
		members = state.Members
	}
	return len(members), true
}

// extractEventCount reads the events_used from tenant metadata quotas.
// This is the FALLBACK source used only when live Core stats are unavailable;
// the metadata.quotas.events_used mirror is kept current by Core's usage-metering
// increment path (apps/core POST /tenants/{id}/usage/increment).
func extractEventCount(t *entities.Tenant) int64 {
	if t.Metadata == nil {
		return 0
	}
	quotas, ok := t.Metadata["quotas"]
	if !ok {
		return 0
	}
	quotaMap, ok := quotas.(map[string]interface{})
	if !ok {
		return 0
	}
	count, ok := quotaMap["events_used"]
	if !ok {
		return 0
	}
	switch v := count.(type) {
	case float64:
		return int64(v)
	case int64:
		return v
	case int:
		return int64(v)
	default:
		return 0
	}
}

// extractMemberCount reads the member_count from tenant metadata.
func extractMemberCount(t *entities.Tenant) int {
	if t.Metadata == nil {
		return 0
	}
	count, ok := t.Metadata["member_count"]
	if !ok {
		return 0
	}
	switch v := count.(type) {
	case float64:
		return int(v)
	case int:
		return v
	case int64:
		return int(v)
	default:
		return 0
	}
}
