package entities

import (
	"strings"
	"time"
)

// SubscriptionTier represents a billing tier.
type SubscriptionTier string

// Subscription tier constants.
//
// CANONICAL ladder (prompt 011 / docs/proposals/PRICING_EXPOSURE_PLAN.md §2):
//
//	free (Self-Host, no hosted checkout) → indie → studio → scale → enterprise
//
// The public marketing ids in apps/web/src/lib/config.ts map to these backend
// tiers via `billingTier` (self-host→free, indie→indie, studio→studio,
// scale→scale, enterprise→enterprise). Note: 010's config.ts still maps
// indie→"starter" / studio→"growth"; 011 introduces first-class indie/studio/
// scale backend tiers AND keeps the legacy tiers as aliases so no in-flight
// subscription or stored tenant metadata breaks during cutover.
//
// RETIRED tiers (pre-011) kept ONLY as aliases for backwards compatibility with
// stored tenant metadata and in-flight webhook payloads — NEVER offer these in a
// new checkout. Map them with MapRetiredTier before applying entitlements:
//
//	developer/starter → indie
//	pro               → indie   (closest paid successor: hosted read, x402)
//	growth/team       → studio
const (
	// Canonical 011 tiers.
	TierFree       SubscriptionTier = "free"       // Self-Host. No hosted checkout.
	TierIndie      SubscriptionTier = "indie"      // $19/mo, 500K events/mo, 50K x402, MCP read
	TierStudio     SubscriptionTier = "studio"     // $79/mo, 5M events/mo, 500K x402, MCP read+write
	TierScale      SubscriptionTier = "scale"      // $299/mo, 50M events/mo, 5M x402, MCP read+write+dedicated
	TierEnterprise SubscriptionTier = "enterprise" // Custom, unlimited

	// Retired aliases — kept so old payloads / stored metadata resolve. Do NOT
	// surface in new checkouts. See MapRetiredTier.
	TierPro     SubscriptionTier = "pro"     // RETIRED: $29/mo. Successor: indie.
	TierGrowth  SubscriptionTier = "growth"  // RETIRED: $79/$99/mo. Successor: studio.
	TierTeam    SubscriptionTier = "team"    // RETIRED: legacy alias for growth → studio.
	TierStarter SubscriptionTier = "starter" // RETIRED: 010 billingTier for indie → indie.
)

// retiredTierMap maps a retired/legacy tier id to its canonical 011 successor.
// Used at read time (entitlement resolution) and by the cutover backfill so no
// existing subscription points at a deleted price/entitlement set.
var retiredTierMap = map[SubscriptionTier]SubscriptionTier{
	"developer": TierIndie,
	TierStarter: TierIndie,
	TierPro:     TierIndie,
	TierGrowth:  TierStudio,
	TierTeam:    TierStudio,
}

// MapRetiredTier returns the canonical 011 tier for a (possibly retired) tier
// id. Canonical tiers pass through unchanged. Unknown tiers pass through too so
// the caller can decide how to handle them (QuotasForTier falls back to free).
func MapRetiredTier(tier string) string {
	if mapped, ok := retiredTierMap[SubscriptionTier(tier)]; ok {
		return string(mapped)
	}
	return tier
}

// SubscriptionMetadata holds billing/subscription data stored in Core tenant metadata.
type SubscriptionMetadata struct {
	CustomerID         string     `json:"customer_id,omitempty"`
	SubscriptionID     string     `json:"subscription_id,omitempty"`
	SubscriptionItemID string     `json:"subscription_item_id,omitempty"` // LemonSqueezy subscription item for metered billing
	Status             string     `json:"status,omitempty"`               // active, past_due, canceled, trialing, expired
	Tier               string     `json:"tier"`
	BillingPeriod      string     `json:"billing_period,omitempty"`   // "monthly" or "annual" — set by 011 checkout
	PaymentProvider    string     `json:"payment_provider,omitempty"` // "lemonsqueezy" (or "comp" for vouchers)
	TrialEndsAt        *time.Time `json:"trial_ends_at,omitempty"`
	SubscriptionEndsAt *time.Time `json:"subscription_ends_at,omitempty"`
	// GrandfatherUntil, when set, marks a tenant whose access is preserved past
	// a pricing change until this date even though their tier no longer offers
	// hosted access (e.g. free/Self-Host tenants kept for 90 days post-011
	// cutover — PRICING_EXPOSURE_PLAN.md §6). Nil = not grandfathered.
	GrandfatherUntil *time.Time `json:"grandfather_until,omitempty"`
}

// IsGrandfathered reports whether the tenant is currently inside its
// grandfather window (GrandfatherUntil set and in the future).
func (s SubscriptionMetadata) IsGrandfathered(now time.Time) bool {
	return s.GrandfatherUntil != nil && now.Before(*s.GrandfatherUntil)
}

// MCP scope values, gated by tier per PRICING_EXPOSURE_PLAN.md §2.
const (
	MCPScopeNone          = ""                     // Self-Host runs its own; no hosted MCP entitlement
	MCPScopeRead          = "read"                 // Indie
	MCPScopeReadWrite     = "read+write"           // Studio
	MCPScopeReadWriteDedi = "read+write+dedicated" // Scale
	MCPScopeDedicated     = "dedicated"            // Enterprise (negotiated cluster)
)

// QuotaMetadata holds usage quota data stored in Core tenant metadata.
type QuotaMetadata struct {
	EventsQuota  int64  `json:"events_quota"`
	QueriesQuota int64  `json:"queries_quota"`
	EventsUsed   int64  `json:"events_used"`
	QueriesUsed  int64  `json:"queries_used"`
	ResetDate    string `json:"reset_date,omitempty"` // ISO-8601 date for next reset

	// --- 011 entitlements ---
	// X402Allowance is the number of x402 calls included per billing period
	// before pay-as-you-go overage applies. 0 = no included x402 allowance.
	X402Allowance int64 `json:"x402_allowance,omitempty"`
	// X402Used is the count of x402 calls consumed this period (metered from
	// settled x402 payment events; see x402.AllowanceChecker).
	X402Used int64 `json:"x402_used,omitempty"`
	// ExtractionTokensQuota is the included hosted Hound extraction LLM tokens per
	// billing period for the tier (-1 = unlimited, 0 = none). Entitlement, not usage.
	ExtractionTokensQuota int64 `json:"extraction_tokens_quota,omitempty"`
	// ExtractionTokensUsed is the LLM tokens consumed this period by hosted Hound
	// doc extraction, metered from prime.extraction.usage events in Core (see
	// billing.SyncExtractionUsageUseCase). Billed past the allowance by
	// billing.ReportExtractionOverageUseCase once a rate is configured.
	ExtractionTokensUsed int64 `json:"extraction_tokens_used,omitempty"`
	// RetentionDays is the event retention window for the tier. -1 = forever.
	RetentionDays int64 `json:"retention_days,omitempty"`
	// MaxStreams is the allowed stream count. -1 = unlimited.
	MaxStreams int64 `json:"max_streams,omitempty"`
	// MCPScope gates hosted MCP verbs (see MCPScope* constants).
	MCPScope string `json:"mcp_scope,omitempty"`
}

// OverageMetadata holds overage billing data stored in Core tenant metadata.
type OverageMetadata struct {
	Enabled             bool    `json:"enabled"`
	EventRate           float64 `json:"event_rate,omitempty"` // cost per event over quota
	QueryRate           float64 `json:"query_rate,omitempty"` // cost per query over quota
	EventsOverage       int64   `json:"events_overage"`
	QueriesOverage      int64   `json:"queries_overage"`
	LastReportedEvents  int64   `json:"last_reported_events,omitempty"`  // last reported overage to prevent double-reporting
	LastReportedQueries int64   `json:"last_reported_queries,omitempty"` // last reported overage to prevent double-reporting
	// LastReportedExtractionUnits is the extraction overage, in billed units,
	// already reported for ExtractionReportedPeriod. The extraction meter resets
	// each period, so a stale period means nothing has been reported yet.
	LastReportedExtractionUnits int64  `json:"last_reported_extraction_units,omitempty"`
	ExtractionReportedPeriod    string `json:"extraction_reported_period,omitempty"`
}

// TenantBillingMetadata is the top-level structure for all billing-related
// metadata stored in Core's tenant metadata map.
type TenantBillingMetadata struct {
	Subscription *SubscriptionMetadata `json:"subscription,omitempty"`
	Quotas       *QuotaMetadata        `json:"quotas,omitempty"`
	Overage      *OverageMetadata      `json:"overage,omitempty"`
	// Subscriptions tracks every LemonSqueezy subscription seen for the tenant,
	// keyed by subscription id. The tenant's effective tier is the highest-ranked
	// tier among the active ones (HighestActiveTier) — so duplicate subscriptions
	// "bubble up" to the most-paid plan, and canceling the top one falls back to
	// the next active. nil when there are no tracked subscriptions.
	Subscriptions map[string]SubscriptionRef `json:"subscriptions,omitempty"`
}

// SubscriptionRef is the per-subscription record in TenantBillingMetadata.Subscriptions.
type SubscriptionRef struct {
	Tier            string `json:"tier"`
	Status          string `json:"status"`
	CustomerID      string `json:"customer_id,omitempty"`
	BillingPeriod   string `json:"billing_period,omitempty"`
	PaymentProvider string `json:"payment_provider,omitempty"`
}

// SubscriptionIsActive reports whether a status grants entitlements (incl.
// past_due, which keeps access during the dunning grace window).
func SubscriptionIsActive(status string) bool {
	switch strings.ToLower(status) {
	case "active", "on_trial", "trialing", "past_due":
		return true
	default:
		return false
	}
}

// tierRank orders tiers for "highest wins"; retired aliases map to successors first.
func tierRank(tier string) int {
	//nolint:exhaustive // retired tiers (pro/growth/team/starter) are normalized
	// to their successors by MapRetiredTier before this switch; default covers the rest.
	switch SubscriptionTier(MapRetiredTier(tier)) {
	case TierEnterprise:
		return 5
	case TierScale:
		return 4
	case TierStudio:
		return 3
	case TierIndie:
		return 2
	case TierFree:
		return 1
	default:
		return 0
	}
}

// TierRank exposes the canonical tier ordering (free<indie<studio<scale<
// enterprise; retired tiers normalized first; unknown→0) so callers can detect an
// upgrade (rank increase) without re-deriving the ladder. Used by the comms
// efficiency engine to tell a paid upgrade from a first activation.
func TierRank(tier string) int { return tierRank(tier) }

// HighestActiveTier returns the highest-ranked tier among active subscriptions
// and the winning subscription id. Returns ("free", "") when none are active.
func HighestActiveTier(subs map[string]SubscriptionRef) (tier, subscriptionID string) {
	bestTier, bestID, bestRank := string(TierFree), "", -1
	for id, s := range subs {
		if !SubscriptionIsActive(s.Status) {
			continue
		}
		if r := tierRank(s.Tier); r > bestRank {
			bestRank, bestTier, bestID = r, s.Tier, id
		}
	}
	return bestTier, bestID
}

// PrimarySubscriptionFor resolves the effective primary subscription for a
// tenant from its tracked subscriptions map. The primary is the highest-ranked
// ACTIVE subscription (bubble-up); when none are active it returns a canceled
// primary at the "free" tier. This is the single source of truth for
// effective-tier resolution shared by every apply path (webhook, change-plan,
// reconciliation). defaultProvider fills PaymentProvider when the winning ref
// doesn't carry one.
func PrimarySubscriptionFor(subs map[string]SubscriptionRef, defaultProvider string) *SubscriptionMetadata {
	effective, primaryID := HighestActiveTier(subs)
	primary := &SubscriptionMetadata{Tier: effective, Status: "active", PaymentProvider: defaultProvider}
	if primaryID != "" {
		p := subs[primaryID]
		primary.SubscriptionID = primaryID
		primary.CustomerID = p.CustomerID
		primary.BillingPeriod = p.BillingPeriod
		if p.PaymentProvider != "" {
			primary.PaymentProvider = p.PaymentProvider
		}
	} else {
		primary.Status = "canceled" // no active subscriptions remain
	}
	return primary
}

// TierQuotas defines the full entitlement set for a given tier. Beyond the
// original events/queries quotas it now (011) carries the x402 allowance,
// retention window, stream cap, and hosted MCP scope so a single tier lookup
// produces everything the webhook needs to persist on the tenant.
//
// Sentinel: -1 means unlimited / forever.
type TierQuotas struct {
	EventsQuota           int64
	QueriesQuota          int64
	X402Allowance         int64  // included x402 calls per period before overage
	ExtractionTokensQuota int64  // included hosted Hound extraction LLM tokens per period; -1 = unlimited, 0 = none
	RetentionDays         int64  // event retention window; -1 = forever
	MaxStreams            int64  // -1 = unlimited
	MCPScope              string // hosted MCP verbs (MCPScope* constants)
}

// TierQuotaMap maps tiers to their entitlements. Numbers come from
// docs/proposals/PRICING_EXPOSURE_PLAN.md §2 (the canonical 011 source).
//
//	Indie  $19  — 500K events, 50K x402, 14d retention, 3 streams, MCP read
//	Studio $79  — 5M events, 500K x402, 90d retention, unlimited streams, MCP read+write
//	Scale  $299 — 50M events, 5M x402, 365d retention, unlimited streams, MCP read+write+dedicated
//
// Retired tiers are NOT listed here; resolve them through MapRetiredTier first
// (QuotasForTier does this automatically).
var TierQuotaMap = map[SubscriptionTier]TierQuotas{
	TierFree: {
		// Self-Host: no hosted quota path. Kept minimal so a free/grandfathered
		// hosted tenant still resolves to *something* non-nil.
		EventsQuota: 100_000, QueriesQuota: 10_000,
		X402Allowance: 0, ExtractionTokensQuota: 0, RetentionDays: 7, MaxStreams: 1, MCPScope: MCPScopeNone,
	},
	TierIndie: {
		EventsQuota: 500_000, QueriesQuota: 50_000,
		X402Allowance: 50_000, ExtractionTokensQuota: 1_000_000, RetentionDays: 14, MaxStreams: 3, MCPScope: MCPScopeRead,
	},
	TierStudio: {
		EventsQuota: 5_000_000, QueriesQuota: 500_000,
		X402Allowance: 500_000, ExtractionTokensQuota: 10_000_000, RetentionDays: 90, MaxStreams: -1, MCPScope: MCPScopeReadWrite,
	},
	TierScale: {
		EventsQuota: 50_000_000, QueriesQuota: 5_000_000,
		X402Allowance: 5_000_000, ExtractionTokensQuota: 100_000_000, RetentionDays: 365, MaxStreams: -1, MCPScope: MCPScopeReadWriteDedi,
	},
	TierEnterprise: {
		EventsQuota: -1, QueriesQuota: -1,
		X402Allowance: -1, ExtractionTokensQuota: -1, RetentionDays: -1, MaxStreams: -1, MCPScope: MCPScopeDedicated,
	},
}

// legacyEventsFloor preserves the events/queries quota that retired PAID tiers
// carried BEFORE 011, so an existing customer is never silently downgraded on
// the dimension they were already paying for (pro had 1M events, growth 10M).
//
// The new 011 dimensions (x402 allowance, retention, streams, MCP scope) are
// NOT floored here — they didn't exist pre-011, so taking them from the
// successor tier is additive, not a reduction. New signups can never land on a
// retired tier, so this only ever protects existing subscriptions across the
// cutover. See docs/runbooks/PRICING_BILLING_CUTOVER.md.
var legacyEventsFloor = map[SubscriptionTier]TierQuotas{
	TierPro:    {EventsQuota: 1_000_000, QueriesQuota: 100_000},
	TierGrowth: {EventsQuota: 10_000_000, QueriesQuota: 1_000_000},
	TierTeam:   {EventsQuota: 10_000_000, QueriesQuota: 1_000_000},
	// developer/starter resolve to indie, whose quota already meets or exceeds
	// their pre-011 entry/free quota — no floor needed.
}

// QuotasForTier returns the entitlements for the given tier. Retired tier ids
// are mapped to their successor first. A retired PAID tier never drops below
// its pre-011 events/queries quota (no-downgrade floor). Returns free-tier
// entitlements for unknown tiers.
func QuotasForTier(tier string) TierQuotas {
	q, ok := TierQuotaMap[SubscriptionTier(MapRetiredTier(tier))]
	if !ok {
		q = TierQuotaMap[TierFree]
	}
	if floor, ok := legacyEventsFloor[SubscriptionTier(tier)]; ok {
		if floor.EventsQuota > q.EventsQuota {
			q.EventsQuota = floor.EventsQuota
		}
		if floor.QueriesQuota > q.QueriesQuota {
			q.QueriesQuota = floor.QueriesQuota
		}
	}
	return q
}

// IsUnlimited returns true if the quota value represents unlimited usage.
func (q TierQuotas) IsUnlimited() bool {
	return q.EventsQuota < 0 && q.QueriesQuota < 0
}

// ToMetadataMap converts a TenantBillingMetadata to a flat map suitable for
// merging into Core's tenant metadata.
func (b *TenantBillingMetadata) ToMetadataMap() map[string]any {
	m := make(map[string]any)
	if b.Subscription != nil {
		m["subscription"] = b.Subscription
	}
	if b.Quotas != nil {
		m["quotas"] = b.Quotas
	}
	if b.Overage != nil {
		m["overage"] = b.Overage
	}
	if b.Subscriptions != nil {
		m["subscriptions"] = b.Subscriptions
	}
	return m
}
