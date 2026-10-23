package usecases

import (
	"context"
	"fmt"
	"log"

	"github.com/allsource/control-plane/internal/logsafe"

	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/domain/repositories"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// LemonSqueezyWebhookEvent represents a parsed LemonSqueezy webhook payload.
type LemonSqueezyWebhookEvent struct {
	EventName string                 `json:"event_name"`
	Data      LemonSqueezyEventData  `json:"data"`
	Meta      map[string]interface{} `json:"meta"`
}

// LemonSqueezyEventData holds the data object from a LemonSqueezy webhook.
type LemonSqueezyEventData struct {
	ID         string                        `json:"id"`
	Type       string                        `json:"type"`
	Attributes LemonSqueezySubscriptionAttrs `json:"attributes"`
}

// LemonSqueezySubscriptionAttrs holds subscription attributes from LemonSqueezy.
type LemonSqueezySubscriptionAttrs struct {
	StoreID         int    `json:"store_id"`
	CustomerID      int    `json:"customer_id"`
	ProductID       int    `json:"product_id"`
	VariantID       int    `json:"variant_id"`
	Status          string `json:"status"`
	StatusFormatted string `json:"status_formatted"`
	ProductName     string `json:"product_name"`
	VariantName     string `json:"variant_name"`
	UserName        string `json:"user_name"`
	UserEmail       string `json:"user_email"`
}

// Tier constants for subscription plans.
const (
	// Canonical 011 tiers.
	tierIndie  = "indie"
	tierStudio = "studio"
	tierScale  = "scale"
	// Retired tiers — kept so legacy variant names / stored metadata resolve.
	tierGrowth     = "growth"
	tierTeam       = "team"
	tierEnterprise = "enterprise"
)

// VariantTierMap maps LemonSqueezy variant names/IDs to subscription tier names.
// Built as a reverse lookup from the LEMON_SQUEEZY_VARIANT_MAP env var.
type VariantTierMap map[string]string

// ProcessLemonSqueezyWebhookUseCase processes incoming LemonSqueezy webhook events.
type ProcessLemonSqueezyWebhookUseCase struct {
	Prepaid        *EgressPrepaid
	tenantRepo     repositories.TenantRepository
	auditRepo      repositories.AuditRepository
	updateSubUC    *UpdateSubscriptionMetadataUseCase
	suspendUC      *SuspendTenantUseCase
	variantTierMap VariantTierMap
	coreClient     clients.CoreClient
}

// NewProcessLemonSqueezyWebhookUseCase creates a new ProcessLemonSqueezyWebhookUseCase.
func NewProcessLemonSqueezyWebhookUseCase(
	tenantRepo repositories.TenantRepository,
	auditRepo repositories.AuditRepository,
	updateSubUC *UpdateSubscriptionMetadataUseCase,
	suspendUC *SuspendTenantUseCase,
	variantTierMap VariantTierMap,
	coreClient ...clients.CoreClient,
) *ProcessLemonSqueezyWebhookUseCase {
	uc := &ProcessLemonSqueezyWebhookUseCase{
		tenantRepo:     tenantRepo,
		auditRepo:      auditRepo,
		updateSubUC:    updateSubUC,
		suspendUC:      suspendUC,
		variantTierMap: variantTierMap,
	}
	if len(coreClient) > 0 {
		uc.coreClient = coreClient[0]
	}
	return uc
}

// Execute processes a LemonSqueezy webhook event.
func (uc *ProcessLemonSqueezyWebhookUseCase) Execute(ctx context.Context, event LemonSqueezyWebhookEvent) error {
	if event.EventName == "" {
		if name, ok := event.Meta["event_name"].(string); ok {
			event.EventName = name
		}
	}
	if custom, ok := event.Meta["custom_data"].(map[string]interface{}); ok && custom["egress_checkout"] != nil {
		if uc.Prepaid == nil {
			return fmt.Errorf("prepaid processing unavailable")
		}
		return uc.Prepaid.Process(ctx, event)
	}
	tenantID := extractTenantIDFromWebhook(event)
	if tenantID == "" {
		return fmt.Errorf("webhook missing tenant_id in custom_data")
	}

	attrs := event.Data.Attributes
	tier := uc.resolveTier(attrs.VariantName, attrs.VariantID)

	var err error
	switch event.EventName {
	case "subscription_created":
		err = uc.handleSubscriptionCreated(tenantID, event)
	case "subscription_updated":
		err = uc.handleSubscriptionUpdated(tenantID, event)
	case "subscription_cancelled": //nolint:misspell // LemonSqueezy API event name
		err = uc.handleSubscriptionCanceled(ctx, tenantID, event)
	case "subscription_expired":
		err = uc.handleSubscriptionExpired(ctx, tenantID, event)
	case "subscription_payment_failed":
		err = uc.handlePaymentFailed(tenantID, event)
	default:
		// Unknown event — log and ignore
		uc.logAudit("webhook.unknown", "receive", tenantID, event.EventName)
		return nil
	}

	// Write billing event to Core (non-blocking — errors logged but don't fail the webhook)
	if err == nil {
		billingEventType := "billing." + event.EventName
		uc.writeBillingEvent(ctx, billingEventType, tenantID, attrs, tier, attrs.Status)
	}

	if err != nil {
		uc.logAudit("webhook.error", "process", tenantID, fmt.Sprintf("%s: %v", event.EventName, err))
		return err
	}

	uc.logAudit("webhook."+event.EventName, "process", tenantID, event.EventName)
	return nil
}

// applySubscriptionEvent upserts one subscription into the tenant's tracked set
// and recomputes the effective tier as the highest-ranked ACTIVE subscription —
// so duplicate subscriptions bubble up to the most-paid plan, and canceling the
// top one falls back to the next active (or free). Returns the effective tier.
func (uc *ProcessLemonSqueezyWebhookUseCase) applySubscriptionEvent(
	tenantID, subID, tier, status, billingPeriod, customerID string,
) (string, error) {
	// Atomic per-tenant upsert + bubble-up (race-safe; see UpsertSubscription).
	effective, _, err := uc.updateSubUC.UpsertSubscription(tenantID, subID, entities.SubscriptionRef{
		Tier:            tier,
		Status:          status,
		CustomerID:      customerID,
		BillingPeriod:   billingPeriod,
		PaymentProvider: "lemonsqueezy",
	})
	return effective, err
}

func (uc *ProcessLemonSqueezyWebhookUseCase) handleSubscriptionCreated(tenantID string, event LemonSqueezyWebhookEvent) error {
	attrs := event.Data.Attributes
	_, err := uc.applySubscriptionEvent(tenantID, event.Data.ID,
		uc.resolveTier(attrs.VariantName, attrs.VariantID), attrs.Status,
		customDataField(event, "billing_period"), fmt.Sprintf("%d", attrs.CustomerID))
	return err
}

func (uc *ProcessLemonSqueezyWebhookUseCase) handleSubscriptionUpdated(tenantID string, event LemonSqueezyWebhookEvent) error {
	attrs := event.Data.Attributes
	_, err := uc.applySubscriptionEvent(tenantID, event.Data.ID,
		uc.resolveTier(attrs.VariantName, attrs.VariantID), attrs.Status,
		customDataField(event, "billing_period"), fmt.Sprintf("%d", attrs.CustomerID))
	return err
}

func (uc *ProcessLemonSqueezyWebhookUseCase) handleSubscriptionCanceled(ctx context.Context, tenantID string, event LemonSqueezyWebhookEvent) error {
	return uc.handleSubscriptionEnd(ctx, tenantID, event, "cancelled") //nolint:misspell // mirrors LemonSqueezy's "cancelled" status spelling
}

func (uc *ProcessLemonSqueezyWebhookUseCase) handleSubscriptionExpired(ctx context.Context, tenantID string, event LemonSqueezyWebhookEvent) error {
	return uc.handleSubscriptionEnd(ctx, tenantID, event, "expired")
}

// handleSubscriptionEnd marks a subscription inactive, recomputes the effective
// tier, and suspends the tenant ONLY when no active subscription remains (a
// tenant with a second active sub keeps access at that tier).
func (uc *ProcessLemonSqueezyWebhookUseCase) handleSubscriptionEnd(
	ctx context.Context, tenantID string, event LemonSqueezyWebhookEvent, status string,
) error {
	attrs := event.Data.Attributes
	effective, err := uc.applySubscriptionEvent(tenantID, event.Data.ID,
		uc.resolveTier(attrs.VariantName, attrs.VariantID), status,
		customDataField(event, "billing_period"), fmt.Sprintf("%d", attrs.CustomerID))
	if err != nil {
		return fmt.Errorf("update subscription metadata: %w", err)
	}
	if effective == defaultPlan {
		if _, err := uc.suspendUC.Execute(ctx, tenantID, entities.RoleAdmin); err != nil {
			return fmt.Errorf("suspend tenant: %w", err)
		}
	}
	return nil
}

func (uc *ProcessLemonSqueezyWebhookUseCase) handlePaymentFailed(tenantID string, event LemonSqueezyWebhookEvent) error {
	// past_due keeps access during the dunning grace window (no suspend yet).
	attrs := event.Data.Attributes
	_, err := uc.applySubscriptionEvent(tenantID, event.Data.ID,
		uc.resolveTier(attrs.VariantName, attrs.VariantID), "past_due",
		customDataField(event, "billing_period"), fmt.Sprintf("%d", attrs.CustomerID))
	return err
}

// writeBillingEvent writes a billing event to Core for durability and audit trail.
// Errors are logged but don't block webhook processing.
func (uc *ProcessLemonSqueezyWebhookUseCase) writeBillingEvent(ctx context.Context, eventType, tenantID string, attrs LemonSqueezySubscriptionAttrs, tier, status string) {
	if uc.coreClient == nil {
		return
	}

	payload := map[string]any{
		"subscription_id":  "",
		"customer_id":      fmt.Sprintf("%d", attrs.CustomerID),
		"tier":             tier,
		"status":           status,
		"payment_provider": "lemonsqueezy",
		"variant_id":       attrs.VariantID,
		"variant_name":     attrs.VariantName,
		"product_name":     attrs.ProductName,
	}

	_, err := uc.coreClient.IngestEvent(ctx, clients.IngestEventRequest{
		EventType: eventType,
		EntityID:  tenantID,
		Payload:   payload,
	})
	if err != nil {
		log.Printf("[billing] failed to write %s event for tenant %s to Core: %v", logsafe.String(eventType), logsafe.String(tenantID), err)
	}
}

func (uc *ProcessLemonSqueezyWebhookUseCase) logAudit(eventType, action, tenantID, detail string) {
	auditEvent, _ := entities.NewAuditEvent(eventType, action, "POST", "/webhooks/lemonsqueezy") //nolint:errcheck
	auditEvent.WithResource("webhook", tenantID).WithTenant(tenantID)
	auditEvent.AddMetadata("detail", detail)
	_ = uc.auditRepo.Log(auditEvent) //nolint:errcheck
}

// customDataField pulls a string field from the webhook's meta.custom_data.
// LemonSqueezy echoes the checkout's custom_data (tenant_id, tier,
// billing_period) on subscription webhooks.
func customDataField(event LemonSqueezyWebhookEvent, key string) string {
	meta := event.Meta
	if meta == nil {
		return ""
	}
	customData, ok := meta["custom_data"]
	if !ok {
		return ""
	}
	switch cd := customData.(type) {
	case map[string]interface{}:
		if v, ok := cd[key].(string); ok {
			return v
		}
	case map[string]string:
		return cd[key]
	}
	return ""
}

// extractTenantIDFromWebhook pulls the tenant_id from the webhook's meta.custom_data.
func extractTenantIDFromWebhook(event LemonSqueezyWebhookEvent) string {
	return customDataField(event, "tenant_id")
}

// resolveTier maps a LemonSqueezy variant name or ID to a tier using the variant map.
// Falls back to hardcoded mapping if variant map is not configured, and "free" as last resort.
// resolveTier maps a LemonSqueezy variant to an internal tier. variant_id is
// the SOLE authority for live webhooks: LEMON_SQUEEZY_VARIANT_MAP (reversed to
// variantID→tier in the container) is the single source of truth.
//
// Live LS variant *names* are "Monthly"/"Annual"/"Default" and carry no tier
// signal, so the name path below is NOT used by real webhooks. It exists only
// for (a) manual HMAC-signed replays, which inject a synthetic variant_name
// like "Studio", and (b) retired tiers that predate the env map. When it fires
// for what should be a live webhook, the variant is missing from
// LEMON_SQUEEZY_VARIANT_MAP — a config gap we log so it gets noticed.
func (uc *ProcessLemonSqueezyWebhookUseCase) resolveTier(variantName string, variantID int) string {
	// Authoritative: variant_id via the env-configured map.
	if uc.variantTierMap != nil && variantID != 0 {
		if tier, ok := uc.variantTierMap[fmt.Sprintf("%d", variantID)]; ok {
			// Canonicalize at this ingest edge so stored tiers are always the
			// canonical id — the single retired→canonical map lives here.
			return entities.MapRetiredTier(tier)
		}
	}

	// Replay-only / legacy-name fallback. Never the path for a live webhook.
	tier := resolveTierByName(variantName)
	if tier != defaultPlan {
		log.Printf("resolveTier: variant_id %d not in LEMON_SQUEEZY_VARIANT_MAP; "+
			"resolved tier %q from variant_name %q (manual replay or config gap)",
			variantID, logsafe.String(tier), logsafe.String(variantName))
	}
	return entities.MapRetiredTier(tier)
}

// resolveTierByName resolves a tier from a variant NAME. Replay/legacy only —
// live LS variant names ("Monthly"/"Annual"/"Default") never match here; see
// resolveTier. Returns defaultPlan ("free") for anything unrecognized.
func resolveTierByName(variantName string) string {
	switch variantName {
	case "Indie", tierIndie:
		return tierIndie
	case "Studio", tierStudio:
		return tierStudio
	case "Scale", tierScale:
		return tierScale
	case "Pro", "pro", "Growth", tierGrowth:
		return tierGrowth
	case "Team", tierTeam:
		return tierTeam
	case "Enterprise", tierEnterprise:
		return tierEnterprise
	default:
		return defaultPlan
	}
}
