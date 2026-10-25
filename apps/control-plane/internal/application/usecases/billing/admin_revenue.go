package billing

import (
	"context"
	"fmt"
	"sort"
	"time"

	"github.com/allsource/control-plane/internal/application/dto"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/domain/repositories"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// Subscription status and tier constants used across billing logic.
const (
	statusPastDue = "past_due"
	tierFree      = "free"
)

// TierPriceMap maps subscription tiers to their monthly price in cents. Keyed
// by canonical 011 tiers (the values stored after ingest canonicalization) plus
// retired aliases for historical rows that predate the cutover — otherwise a
// tenant on a canonical tier (indie/studio/scale) was valued at $0.
var TierPriceMap = map[string]int{
	tierFree:     0,
	"self-host":  0,
	"indie":      1900,  // $19/mo (011)
	"studio":     7900,  // $79/mo (011)
	"scale":      29900, // $299/mo (011)
	"enterprise": 0,     // negotiated; not derivable from a flat price
	// Retired 010 tiers — kept for historical revenue rows.
	"starter": 1900,
	"pro":     2900, // $29/mo
	"growth":  7900,
	"team":    9900, // $99/mo
}

// TenantPlan returns a tenant's subscription tier, status and list price in
// US dollars per month. An enterprise price is negotiated, so it reads as 0.
func TenantPlan(t *entities.Tenant) (tier, status string, monthlyUSD float64) {
	sub := extractSubscriptionEntity(t.Metadata)
	tier = sub.Tier
	if tier == "" {
		tier = tierFree
	}
	return tier, sub.Status, float64(TierPriceMap[tier]) / 100.0
}

// AdminRevenueUseCase computes revenue metrics from subscription data.
type AdminRevenueUseCase struct {
	tenantRepo repositories.TenantRepository
	auditRepo  repositories.AuditRepository
	lsClient   clients.LemonSqueezyClient
}

// NewAdminRevenueUseCase creates a new AdminRevenueUseCase.
func NewAdminRevenueUseCase(
	tenantRepo repositories.TenantRepository,
	auditRepo repositories.AuditRepository,
	lsClient clients.LemonSqueezyClient,
) *AdminRevenueUseCase {
	return &AdminRevenueUseCase{
		tenantRepo: tenantRepo,
		auditRepo:  auditRepo,
		lsClient:   lsClient,
	}
}

// Execute computes revenue metrics for the given time range.
func (uc *AdminRevenueUseCase) Execute(ctx context.Context, req dto.AdminRevenueRequest, adminUserID string) (*dto.AdminRevenueResponse, error) {
	months := rangeToMonths(req.Range)

	tenants, err := uc.tenantRepo.FindAll()
	if err != nil {
		return nil, fmt.Errorf("find tenants: %w", err)
	}

	result := uc.calculateRevenue(tenants, months)
	result.Period = req.Range

	// Audit log
	auditEvent, _ := entities.NewAuditEvent("admin.billing.revenue.query", "query", "GET", "/api/v1/admin/billing/revenue") //nolint:errcheck
	auditEvent.WithUser(adminUserID, "").WithResource("revenue", "")
	auditEvent.AddMetadata("range", req.Range)
	_ = uc.auditRepo.Log(auditEvent) //nolint:errcheck

	return result, nil
}

// calculateRevenue computes MRR, ARR, growth, and churn from tenant subscription data.
// This is a pure function suitable for unit testing.
func (uc *AdminRevenueUseCase) calculateRevenue(tenants []*entities.Tenant, months int) *dto.AdminRevenueResponse {
	return CalculateRevenueMetrics(tenants, months)
}

// CalculateRevenueMetrics computes MRR, ARR, growth, and churn from tenant subscription data.
// Exported for unit testing.
func CalculateRevenueMetrics(tenants []*entities.Tenant, months int) *dto.AdminRevenueResponse {
	now := time.Now()
	cutoff := now.AddDate(0, -months, 0)

	// Current active subscriptions for MRR
	var activeCount int
	var mrrCents int
	tierStats := make(map[string]dto.TierStats)
	var canceledInPeriod int
	var totalAtPeriodStart int

	for _, t := range tenants {
		sub := extractSubscriptionEntity(t.Metadata)
		tier := sub.Tier
		if tier == "" {
			tier = tierFree
		}

		price := TierPriceMap[tier]

		// Track tier stats for currently active/past_due subs
		switch sub.Status {
		case "active", statusPastDue, "trialing", "":
			activeCount++
			mrrCents += price
			stats := tierStats[tier]
			stats.Count++
			stats.Revenue += float64(price) / 100.0
			tierStats[tier] = stats
		}

		// Count churn: subscriptions that ended/canceled within the period
		if sub.SubscriptionEndsAt != nil && sub.SubscriptionEndsAt.After(cutoff) && sub.SubscriptionEndsAt.Before(now) {
			canceledInPeriod++
		}

		// Estimate subscribers at period start (active + those who churned)
		if t.CreatedAt.Before(cutoff) {
			if sub.Status == "active" || sub.Status == statusPastDue || sub.Status == "trialing" || sub.Status == "" {
				totalAtPeriodStart++
			}
			if sub.SubscriptionEndsAt != nil && sub.SubscriptionEndsAt.After(cutoff) {
				totalAtPeriodStart++
			}
		}
	}

	mrr := float64(mrrCents) / 100.0
	arr := mrr * 12.0

	// Growth rate: (current - start) / start * 100
	var growthRate float64
	if totalAtPeriodStart > 0 {
		growthRate = float64(activeCount-totalAtPeriodStart) / float64(totalAtPeriodStart) * 100.0
	}

	// Churn rate: canceled in period / total at period start * 100
	var churnRate float64
	if totalAtPeriodStart > 0 {
		churnRate = float64(canceledInPeriod) / float64(totalAtPeriodStart) * 100.0
	}

	// Build monthly breakdown
	breakdown := buildMonthlyBreakdown(tenants, months)

	return &dto.AdminRevenueResponse{
		MRR:        mrr,
		ARR:        arr,
		GrowthRate: growthRate,
		ChurnRate:  churnRate,
		Breakdown:  breakdown,
		TierStats:  tierStats,
	}
}

// buildMonthlyBreakdown builds per-month revenue data.
func buildMonthlyBreakdown(tenants []*entities.Tenant, months int) []dto.RevenueBreakdown {
	now := time.Now()
	var breakdown []dto.RevenueBreakdown

	for i := months - 1; i >= 0; i-- {
		monthStart := time.Date(now.Year(), now.Month()-time.Month(i), 1, 0, 0, 0, 0, time.UTC)
		monthEnd := monthStart.AddDate(0, 1, 0)
		period := monthStart.Format("2006-01")

		var revenue float64
		var subscribers int
		var newCustomers int
		var churned int

		for _, t := range tenants {
			sub := extractSubscriptionEntity(t.Metadata)
			tier := sub.Tier
			if tier == "" {
				tier = tierFree
			}

			// Was this tenant active during this month?
			createdBefore := t.CreatedAt.Before(monthEnd)
			notEndedYet := sub.SubscriptionEndsAt == nil || sub.SubscriptionEndsAt.After(monthStart)

			if createdBefore && notEndedYet {
				price := TierPriceMap[tier]
				revenue += float64(price) / 100.0
				subscribers++
			}

			// New customer this month?
			if !t.CreatedAt.Before(monthStart) && t.CreatedAt.Before(monthEnd) {
				newCustomers++
			}

			// Churned this month?
			if sub.SubscriptionEndsAt != nil && !sub.SubscriptionEndsAt.Before(monthStart) && sub.SubscriptionEndsAt.Before(monthEnd) {
				churned++
			}
		}

		breakdown = append(breakdown, dto.RevenueBreakdown{
			Period:       period,
			Revenue:      revenue,
			Subscribers:  subscribers,
			NewCustomers: newCustomers,
			Churned:      churned,
		})
	}

	sort.Slice(breakdown, func(i, j int) bool {
		return breakdown[i].Period < breakdown[j].Period
	})

	return breakdown
}

// extractSubscriptionEntity extracts subscription metadata from a tenant.
func extractSubscriptionEntity(metadata map[string]interface{}) entities.SubscriptionMetadata {
	if metadata == nil {
		return entities.SubscriptionMetadata{}
	}
	raw, ok := metadata["subscription"]
	if !ok {
		return entities.SubscriptionMetadata{}
	}
	switch v := raw.(type) {
	case *entities.SubscriptionMetadata:
		return *v
	case entities.SubscriptionMetadata:
		return v
	case map[string]interface{}:
		return subscriptionFromMap(v)
	default:
		return entities.SubscriptionMetadata{}
	}
}

// rangeToMonths converts a range string to a number of months.
func rangeToMonths(r string) int {
	switch r {
	case "90d":
		return 3
	case "1y":
		return 12
	default: // "30d" or anything else
		return 1
	}
}
