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

// eventExtractionUsage is the event Hound's doc extractor emits once per run with
// the LLM token totals it consumed (see apps/prime-mcp src/doc_extract.rs
// emit_usage). Each event's payload carries total_tokens.
const eventExtractionUsage = "prime.extraction.usage"

// SyncExtractionUsageUseCase reconciles a tenant's extraction_tokens_used meter
// from the prime.extraction.usage events recorded in Core since the start of the
// current billing period. This is the metering half of hosted Hound doc
// extraction: it keeps a truthful per-tenant count of LLM tokens consumed, from
// the durable event log (per CLAUDE.md, event-shaped usage metering routes
// through Core, not an in-memory counter).
//
// It records only; ReportExtractionOverageUseCase turns the meter into a charge.
type SyncExtractionUsageUseCase struct {
	tenantRepo repositories.TenantRepository
	auditRepo  repositories.AuditRepository
	coreClient clients.CoreClient
}

// NewSyncExtractionUsageUseCase creates a new SyncExtractionUsageUseCase.
func NewSyncExtractionUsageUseCase(
	tenantRepo repositories.TenantRepository,
	auditRepo repositories.AuditRepository,
	coreClient clients.CoreClient,
) *SyncExtractionUsageUseCase {
	return &SyncExtractionUsageUseCase{
		tenantRepo: tenantRepo,
		auditRepo:  auditRepo,
		coreClient: coreClient,
	}
}

// ExtractionUsageResult holds the outcome of reconciling one tenant.
type ExtractionUsageResult struct {
	TenantID             string
	ExtractionTokensUsed int64
	Skipped              bool
	Error                error
}

// Execute reconciles extraction_tokens_used for a single tenant.
func (uc *SyncExtractionUsageUseCase) Execute(ctx context.Context, tenantID string) (*ExtractionUsageResult, error) {
	if uc.coreClient == nil {
		return &ExtractionUsageResult{TenantID: tenantID, Skipped: true}, nil
	}

	tenant, err := uc.tenantRepo.FindByID(tenantID)
	if err != nil {
		return nil, err
	}

	quotas := extractQuotas(tenant.Metadata)
	since := periodStart(quotas, time.Now().UTC())

	tokens, err := uc.sumTokens(ctx, tenantID, since)
	if err != nil {
		return nil, fmt.Errorf("sum extraction tokens for %s: %w", tenantID, err)
	}

	if tokens == quotas.ExtractionTokensUsed {
		return &ExtractionUsageResult{TenantID: tenantID, ExtractionTokensUsed: tokens, Skipped: true}, nil
	}

	quotas.ExtractionTokensUsed = tokens
	if tenant.Metadata == nil {
		tenant.Metadata = make(map[string]interface{})
	}
	tenant.Metadata["quotas"] = &quotas

	if err := uc.tenantRepo.Update(tenant); err != nil {
		return nil, fmt.Errorf("persist extraction_tokens_used for %s: %w", tenantID, err)
	}

	if uc.auditRepo != nil {
		auditEvent, _ := entities.NewAuditEvent("billing.extraction.synced", "report", "SCHEDULER", "/billing/extraction") //nolint:errcheck
		auditEvent.WithResource("tenant", tenantID).WithTenant(tenantID)
		auditEvent.AddMetadata("extraction_tokens_used", fmt.Sprintf("%d", tokens))
		_ = uc.auditRepo.Log(auditEvent) //nolint:errcheck
	}

	return &ExtractionUsageResult{TenantID: tenantID, ExtractionTokensUsed: tokens}, nil
}

// sumTokens sums payload.total_tokens across prime.extraction.usage events for
// the tenant since the period start, paging through Core's event query.
func (uc *SyncExtractionUsageUseCase) sumTokens(ctx context.Context, tenantID, since string) (int64, error) {
	var total int64
	offset := 0
	for {
		resp, err := uc.coreClient.QueryEvents(ctx, clients.QueryEventsRequest{
			EventType: eventExtractionUsage,
			TenantID:  tenantID,
			Since:     since,
			Limit:     queryPageLimit,
			Offset:    offset,
		})
		if err != nil {
			return 0, err
		}
		for _, e := range resp.Events {
			if v, ok := e.Payload["total_tokens"].(float64); ok {
				total += int64(v)
			}
		}
		if len(resp.Events) < queryPageLimit {
			break
		}
		offset += queryPageLimit
	}
	return total, nil
}

// ExecuteAll reconciles extraction_tokens_used for all active tenants.
func (uc *SyncExtractionUsageUseCase) ExecuteAll(ctx context.Context) []ExtractionUsageResult {
	tenants, err := uc.tenantRepo.FindActive()
	if err != nil {
		log.Printf("SyncExtractionUsage: failed to list tenants: %v", err)
		return nil
	}

	var results []ExtractionUsageResult
	for _, t := range tenants {
		result, err := uc.Execute(ctx, t.ID)
		if err != nil {
			results = append(results, ExtractionUsageResult{TenantID: t.ID, Error: err})
			continue
		}
		results = append(results, *result)
	}
	return results
}
