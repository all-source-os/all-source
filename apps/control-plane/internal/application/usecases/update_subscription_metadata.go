package usecases

import (
	"context"
	"log"
	"sync"
	"time"

	"github.com/allsource/control-plane/internal/application/dto"
	"github.com/allsource/control-plane/internal/application/usecases/signals"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/domain/repositories"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// keyedMutex serializes work per string key (here: per tenant ID). Lazily
// allocates one mutex per key; entries are never reclaimed, which is fine for a
// bounded tenant set.
type keyedMutex struct {
	m sync.Map
}

func (k *keyedMutex) lock(key string) func() {
	mu, _ := k.m.LoadOrStore(key, &sync.Mutex{})
	mtx := mu.(*sync.Mutex) //nolint:forcetypeassert,errcheck // LoadOrStore above only ever stores *sync.Mutex
	mtx.Lock()
	return mtx.Unlock
}

// UpdateSubscriptionMetadataUseCase writes billing/subscription metadata
// to Core's tenant metadata. Called by the Control Plane when subscription
// events arrive from LemonSqueezy webhooks.
//
// Tenant metadata lives in Core as a single JSON blob updated via
// read-modify-write (FindByID -> mutate -> Update replaces the whole map).
// Concurrent subscription events for the same tenant (e.g. two webhooks, or a
// webhook racing a change-plan / reconciliation) would otherwise lost-update
// each other's entry. The per-tenant lock serializes the full read-modify-write
// so each caller observes the previous write. Single-instance correct (the
// Control Plane runs one Fly machine today); horizontal scale-out would need a
// Core-side compare-and-swap / ETag instead — see t-cad0f0.
type UpdateSubscriptionMetadataUseCase struct {
	tenantRepo repositories.TenantRepository
	auditRepo  repositories.AuditRepository
	locks      *keyedMutex
	// coreClient is optional (set via WithCoreClient). When wired, the apply path
	// emits a subscription.activated / subscription.upgraded Core event on a tier
	// transition so the comms-efficiency engine can measure trial→paid conversion
	// (prompt 050). nil → the emit is a no-op; entitlement writes are unaffected.
	coreClient clients.CoreClient
}

// NewUpdateSubscriptionMetadataUseCase creates a new UpdateSubscriptionMetadataUseCase.
func NewUpdateSubscriptionMetadataUseCase(
	tenantRepo repositories.TenantRepository,
	auditRepo repositories.AuditRepository,
) *UpdateSubscriptionMetadataUseCase {
	return &UpdateSubscriptionMetadataUseCase{
		tenantRepo: tenantRepo,
		auditRepo:  auditRepo,
		locks:      &keyedMutex{},
	}
}

// WithCoreClient wires the Core client so a tier transition emits a durable
// subscription.activated / subscription.upgraded event (the trial→paid signal the
// comms-efficiency reconciler joins against). Returns the receiver for chaining.
// Kept as an optional builder so the existing 2-arg constructor — called in ~17
// places — is untouched.
func (uc *UpdateSubscriptionMetadataUseCase) WithCoreClient(c clients.CoreClient) *UpdateSubscriptionMetadataUseCase {
	uc.coreClient = c
	return uc
}

// UpsertSubscription atomically upserts a single subscription into the tenant's
// tracked set and recomputes the effective tier as the highest-ranked ACTIVE
// subscription. The fresh read of the subscriptions map happens INSIDE the
// per-tenant lock, so concurrent upserts can't lose each other's entry (the
// bug a read-outside-lock + full-map-replace would cause). Returns the
// effective tier and the primary (highest-active) subscription ID.
//
// This is the single apply path for the bubble-up logic shared by the webhook,
// change-plan, and reconciliation use cases.
func (uc *UpdateSubscriptionMetadataUseCase) UpsertSubscription(
	tenantID, subID string, ref entities.SubscriptionRef,
) (effectiveTier, primarySubID string, err error) {
	unlock := uc.locks.lock(tenantID)
	defer unlock()

	tenant, err := uc.tenantRepo.FindByID(tenantID)
	if err != nil {
		return "", "", err
	}
	subs := extractSubscriptionsMap(tenant.Metadata)
	if subs == nil {
		subs = map[string]entities.SubscriptionRef{}
	}
	subs[subID] = ref

	// Single source of truth for effective-tier / primary resolution (domain).
	primary := entities.PrimarySubscriptionFor(subs, providerLemonSqueezy)
	if _, err := uc.applyLocked(tenant, &entities.TenantBillingMetadata{
		Subscription:  primary,
		Subscriptions: subs,
	}); err != nil {
		return "", "", err
	}
	return primary.Tier, primary.SubscriptionID, nil
}

// Execute updates the subscription, quotas, and overage metadata for a tenant.
// It merges billing fields into the existing metadata without overwriting
// non-billing fields.
func (uc *UpdateSubscriptionMetadataUseCase) Execute(tenantID string, billing *entities.TenantBillingMetadata) (*dto.TenantResponse, error) {
	// Serialize the full read-modify-write per tenant (see type doc).
	unlock := uc.locks.lock(tenantID)
	defer unlock()

	// Find existing tenant
	tenant, err := uc.tenantRepo.FindByID(tenantID)
	if err != nil {
		return nil, err
	}
	return uc.applyLocked(tenant, billing)
}

// applyLocked merges billing metadata into an already-fetched tenant and
// persists it. Caller MUST already hold the per-tenant lock and have fetched
// `tenant` inside that lock.
func (uc *UpdateSubscriptionMetadataUseCase) applyLocked(tenant *entities.Tenant, billing *entities.TenantBillingMetadata) (*dto.TenantResponse, error) {
	tenantID := tenant.ID

	// Initialize metadata map if nil
	if tenant.Metadata == nil {
		tenant.Metadata = make(map[string]interface{})
	}

	// Capture the PRE-merge tier state so we can detect a trial→paid (or paid
	// upgrade) transition after persisting and emit the comms-efficiency signal.
	prevTier := effectiveBillingTier(tenant.Metadata, extractSubscriptionForHealth(tenant.Metadata))
	prevPaid := signals.PaidTier(prevTier)
	prevTrial := TenantIsActiveTrial(tenant.Metadata)

	// Merge billing metadata into tenant metadata, preserving non-billing keys
	billingMap := billing.ToMetadataMap()
	for k, v := range billingMap {
		tenant.Metadata[k] = v
	}

	// Apply tier-based entitlements if subscription tier is set and quotas are
	// not explicitly provided. This resolves retired tiers to their successor
	// (QuotasForTier handles that) and persists the FULL 011 entitlement set —
	// events/queries quota, x402 allowance, retention, streams, MCP scope — so
	// downstream enforcement (quota gate, x402 allowance checker) has everything
	// it needs from a single tenant read.
	//
	// CRITICAL: preserve the existing USAGE counters (events_used / queries_used /
	// x402_used / reset_date). These are the metered "used this period" numbers the
	// dashboard reads; rebuilding the quotas map from tier limits alone silently
	// zeroed them on every webhook / change-plan / scheduler tick, which is why a
	// backfilled events_used would not survive. Quota LIMITS come from the tier;
	// USAGE carries forward from whatever was already stored.
	if billing.Subscription != nil && billing.Subscription.Tier != "" && billing.Quotas == nil {
		tierQuotas := entities.QuotasForTier(billing.Subscription.Tier)
		prev := extractQuotas(tenant.Metadata)
		tenant.Metadata["quotas"] = &entities.QuotaMetadata{
			EventsQuota:           tierQuotas.EventsQuota,
			QueriesQuota:          tierQuotas.QueriesQuota,
			X402Allowance:         tierQuotas.X402Allowance,
			ExtractionTokensQuota: tierQuotas.ExtractionTokensQuota,
			RetentionDays:         tierQuotas.RetentionDays,
			EgressBytesQuota:      tierQuotas.EgressBytesQuota,
			MaxStreams:            tierQuotas.MaxStreams,
			MCPScope:              tierQuotas.MCPScope,
			// Carry usage forward — do not reset on a tier apply.
			EventsUsed:           prev.EventsUsed,
			QueriesUsed:          prev.QueriesUsed,
			X402Used:             prev.X402Used,
			ExtractionTokensUsed: prev.ExtractionTokensUsed,
			ResetDate:            prev.ResetDate,
		}
	}

	// Persist
	if err := uc.tenantRepo.Update(tenant); err != nil {
		return nil, err
	}

	// Log audit event
	auditEvent, _ := entities.NewAuditEvent("tenant.subscription.updated", "update", "PUT", "/tenants/"+tenantID+"/subscription") //nolint:errcheck
	auditEvent.WithResource("tenant", tenant.ID).WithTenant(tenant.ID)
	_ = uc.auditRepo.Log(auditEvent) //nolint:errcheck

	// Emit the trial→paid / upgrade signal AFTER a successful persist (the state
	// change already happened — this only RECORDS it; it changes no entitlement and
	// moves no money). Best-effort; nil coreClient → no-op.
	if billing.Subscription != nil && billing.Subscription.Tier != "" {
		newTier := effectiveBillingTier(tenant.Metadata, extractSubscriptionForHealth(tenant.Metadata))
		uc.emitTierTransition(tenant.ID, prevTier, newTier, prevPaid, prevTrial)
	}

	return &dto.TenantResponse{
		ID:          tenant.ID,
		Name:        tenant.Name,
		Description: tenant.Description,
		Status:      string(tenant.Status),
		HomeRegion:  tenant.EffectiveHomeRegion(),
		CreatedAt:   tenant.CreatedAt,
		UpdatedAt:   tenant.UpdatedAt,
		Metadata:    tenant.Metadata,
	}, nil
}

// emitTierTransition records a subscription.activated (trial/free → paid: the HERO
// conversion) or subscription.upgraded (paid → higher paid: expansion) Core event
// into the CUSTOMER tenant's own stream, so the comms-efficiency reconciler can
// join "did the welcome/upgrade email cause this?" within the attribution window.
// Idempotent webhooks/renewals at the SAME tier emit nothing; downgrades and
// cancels (newTier not paid) emit nothing.
func (uc *UpdateSubscriptionMetadataUseCase) emitTierTransition(tenantID, prevTier, newTier string, prevPaid, prevTrial bool) {
	if uc.coreClient == nil {
		return
	}
	if !signals.PaidTier(newTier) {
		return // not a paid state (downgrade to free / cancel) — nothing to mark
	}
	var eventType string
	switch {
	case !prevPaid:
		eventType = GoalSubscriptionActivated // trial/free → paid
	case entities.TierRank(newTier) > entities.TierRank(prevTier):
		eventType = GoalSubscriptionUpgraded // paid → higher paid
	default:
		return // same tier (renewal / replayed webhook) or downgrade — no marker
	}
	if _, err := uc.coreClient.IngestEvent(context.Background(), clients.IngestEventRequest{
		EventType: eventType,
		EntityID:  tenantID, // customer stream — the efficiency join reads here
		TenantID:  tenantID,
		Payload: map[string]any{
			"tenant_id":  tenantID,
			"tier":       entities.MapRetiredTier(newTier),
			"prev_tier":  entities.MapRetiredTier(prevTier),
			"from_trial": prevTrial,
			"at":         time.Now().UTC().Format(time.RFC3339Nano),
		},
	}); err != nil {
		log.Printf("UpdateSubscription: emit %s for tenant %s failed: %v", eventType, tenantID, err)
	}
}
