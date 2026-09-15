package billing

import (
	"context"
	"fmt"
	"log"
	"time"

	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/domain/repositories"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// ReportExtractionOverageUseCase bills hosted Hound extraction tokens used past
// the tier's allowance. It reports to the same LemonSqueezy subscription item as
// event overage, so raw tokens cannot be sent there: they would be priced as
// events. tokensPerUnit converts the overage into billed units, and it is the
// billing owner's rate decision — while it is unset (<= 0) nothing is reported.
//
// Reads extraction_tokens_used as SyncExtractionUsageUseCase left it, so run it
// after that sync.
type ReportExtractionOverageUseCase struct {
	tenantRepo    repositories.TenantRepository
	auditRepo     repositories.AuditRepository
	lsClient      clients.LemonSqueezyClient
	tokensPerUnit int64
	now           func() time.Time
}

// NewReportExtractionOverageUseCase creates a ReportExtractionOverageUseCase.
func NewReportExtractionOverageUseCase(
	tenantRepo repositories.TenantRepository,
	auditRepo repositories.AuditRepository,
	lsClient clients.LemonSqueezyClient,
	tokensPerUnit int64,
) *ReportExtractionOverageUseCase {
	return &ReportExtractionOverageUseCase{
		tenantRepo:    tenantRepo,
		auditRepo:     auditRepo,
		lsClient:      lsClient,
		tokensPerUnit: tokensPerUnit,
		now:           time.Now,
	}
}

// ExtractionOverageResult holds the outcome of reporting one tenant.
type ExtractionOverageResult struct {
	TenantID      string
	UnitsReported int64
	Skipped       bool
	Error         error
}

// Execute reports the extraction overage accrued since the last report.
func (uc *ReportExtractionOverageUseCase) Execute(ctx context.Context, tenantID string) (*ExtractionOverageResult, error) {
	skipped := &ExtractionOverageResult{TenantID: tenantID, Skipped: true}
	if uc.tokensPerUnit <= 0 || uc.lsClient == nil {
		return skipped, nil
	}

	tenant, err := uc.tenantRepo.FindByID(tenantID)
	if err != nil {
		return nil, err
	}

	overage := extractOverage(tenant.Metadata)
	quotas := extractQuotas(tenant.Metadata)
	sub := extractSubscription(tenant.Metadata)
	if !overage.Enabled || sub.SubscriptionItemID == "" || quotas.ExtractionTokensQuota < 0 {
		return skipped, nil
	}

	overageTokens := quotas.ExtractionTokensUsed - quotas.ExtractionTokensQuota
	if overageTokens <= 0 {
		return skipped, nil
	}

	period := periodStart(quotas, uc.now().UTC())
	alreadyReported := overage.LastReportedExtractionUnits
	if overage.ExtractionReportedPeriod != period {
		alreadyReported = 0
	}

	// Floor division: a partial unit is billed once it completes, because the
	// total is recomputed from the meter on every run.
	units := overageTokens / uc.tokensPerUnit
	delta := units - alreadyReported
	if delta <= 0 {
		return skipped, nil
	}

	if err := uc.lsClient.ReportUsage(ctx, clients.ReportUsageRequest{
		SubscriptionItemID: sub.SubscriptionItemID,
		Quantity:           int(delta),
		Action:             "increment",
	}); err != nil {
		return nil, fmt.Errorf("report extraction overage for %s: %w", tenantID, err)
	}

	// The charge has been sent. If recording it fails, the next run would send
	// it again, so a failed update is an error rather than a log line.
	overage.LastReportedExtractionUnits = units
	overage.ExtractionReportedPeriod = period
	if tenant.Metadata == nil {
		tenant.Metadata = make(map[string]interface{})
	}
	tenant.Metadata["overage"] = &overage
	if err := uc.tenantRepo.Update(tenant); err != nil {
		return nil, fmt.Errorf("record reported extraction overage for %s (reported %d units): %w", tenantID, delta, err)
	}

	if uc.auditRepo != nil {
		auditEvent, _ := entities.NewAuditEvent("billing.extraction.overage_reported", "report", "SCHEDULER", "/billing/extraction") //nolint:errcheck
		auditEvent.WithResource("tenant", tenantID).WithTenant(tenantID)
		auditEvent.AddMetadata("units_delta", fmt.Sprintf("%d", delta))
		auditEvent.AddMetadata("overage_tokens", fmt.Sprintf("%d", overageTokens))
		auditEvent.AddMetadata("tokens_per_unit", fmt.Sprintf("%d", uc.tokensPerUnit))
		_ = uc.auditRepo.Log(auditEvent) //nolint:errcheck
	}

	return &ExtractionOverageResult{TenantID: tenantID, UnitsReported: delta}, nil
}

// ExecuteAll reports extraction overage for every active tenant.
func (uc *ReportExtractionOverageUseCase) ExecuteAll(ctx context.Context) []ExtractionOverageResult {
	if uc.tokensPerUnit <= 0 {
		return nil
	}
	tenants, err := uc.tenantRepo.FindActive()
	if err != nil {
		log.Printf("ReportExtractionOverage: failed to list tenants: %v", err)
		return nil
	}

	var results []ExtractionOverageResult
	for _, t := range tenants {
		result, err := uc.Execute(ctx, t.ID)
		if err != nil {
			results = append(results, ExtractionOverageResult{TenantID: t.ID, Error: err})
			continue
		}
		results = append(results, *result)
	}
	return results
}
