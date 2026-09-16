// Package billing provides use cases for computing tenant billing overage.
package billing

import (
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/domain/repositories"
)

// OverageResult holds the computed overage for a single tenant.
type OverageResult struct {
	TenantID       string
	EventsOverage  int64
	QueriesOverage int64
	EventsUsed     int64
	QueriesUsed    int64
	EventsQuota    int64
	QueriesQuota   int64
	Enabled        bool
}

// CalculateOverageUseCase computes overage units from tenant metadata.
type CalculateOverageUseCase struct {
	tenantRepo repositories.TenantRepository
}

// NewCalculateOverageUseCase creates a new CalculateOverageUseCase.
func NewCalculateOverageUseCase(tenantRepo repositories.TenantRepository) *CalculateOverageUseCase {
	return &CalculateOverageUseCase{tenantRepo: tenantRepo}
}

// Execute computes overage for a single tenant.
// Returns nil result (no error) if overage is disabled or quotas are unlimited.
func (uc *CalculateOverageUseCase) Execute(tenantID string) (*OverageResult, error) {
	tenant, err := uc.tenantRepo.FindByID(tenantID)
	if err != nil {
		return nil, err
	}

	overage := extractOverage(tenant.Metadata)
	if !overage.Enabled {
		return &OverageResult{
			TenantID: tenantID,
			Enabled:  false,
		}, nil
	}

	quotas := extractQuotas(tenant.Metadata)

	// Unlimited quotas (-1) means no overage possible
	tierQuotas := entities.QuotasForTier(extractSubscriptionTier(tenant.Metadata))
	if tierQuotas.IsUnlimited() {
		return &OverageResult{
			TenantID:     tenantID,
			Enabled:      true,
			EventsQuota:  quotas.EventsQuota,
			QueriesQuota: quotas.QueriesQuota,
			EventsUsed:   quotas.EventsUsed,
			QueriesUsed:  quotas.QueriesUsed,
		}, nil
	}

	var eventsOverage, queriesOverage int64
	if quotas.EventsUsed > quotas.EventsQuota && quotas.EventsQuota > 0 {
		eventsOverage = quotas.EventsUsed - quotas.EventsQuota
	}
	if quotas.QueriesUsed > quotas.QueriesQuota && quotas.QueriesQuota > 0 {
		queriesOverage = quotas.QueriesUsed - quotas.QueriesQuota
	}

	return &OverageResult{
		TenantID:       tenantID,
		EventsOverage:  eventsOverage,
		QueriesOverage: queriesOverage,
		EventsUsed:     quotas.EventsUsed,
		QueriesUsed:    quotas.QueriesUsed,
		EventsQuota:    quotas.EventsQuota,
		QueriesQuota:   quotas.QueriesQuota,
		Enabled:        true,
	}, nil
}

// ExecuteAll computes overage for all active tenants.
func (uc *CalculateOverageUseCase) ExecuteAll() ([]OverageResult, error) {
	tenants, err := uc.tenantRepo.FindActive()
	if err != nil {
		return nil, err
	}

	var results []OverageResult
	for _, t := range tenants {
		result, err := uc.Execute(t.ID)
		if err != nil {
			continue // skip tenants that error, log in caller
		}
		if result != nil && result.Enabled && (result.EventsOverage > 0 || result.QueriesOverage > 0) {
			results = append(results, *result)
		}
	}
	return results, nil
}

// --- Metadata extraction helpers (same pattern as billing.go) ---

func extractOverage(metadata map[string]interface{}) entities.OverageMetadata {
	if metadata == nil {
		return entities.OverageMetadata{}
	}
	raw, ok := metadata["overage"]
	if !ok {
		return entities.OverageMetadata{}
	}
	switch v := raw.(type) {
	case *entities.OverageMetadata:
		return *v
	case entities.OverageMetadata:
		return v
	case map[string]interface{}:
		return overageFromMap(v)
	default:
		return entities.OverageMetadata{}
	}
}

func extractQuotas(metadata map[string]interface{}) entities.QuotaMetadata {
	if metadata == nil {
		return entities.QuotaMetadata{}
	}
	raw, ok := metadata["quotas"]
	if !ok {
		return entities.QuotaMetadata{}
	}
	switch v := raw.(type) {
	case *entities.QuotaMetadata:
		return *v
	case entities.QuotaMetadata:
		return v
	case map[string]interface{}:
		return quotasFromMap(v)
	default:
		return entities.QuotaMetadata{}
	}
}

func extractSubscriptionTier(metadata map[string]interface{}) string {
	if metadata == nil {
		return ""
	}
	raw, ok := metadata["subscription"]
	if !ok {
		return ""
	}
	switch v := raw.(type) {
	case *entities.SubscriptionMetadata:
		return v.Tier
	case entities.SubscriptionMetadata:
		return v.Tier
	case map[string]interface{}:
		if tier, ok := v["tier"].(string); ok {
			return tier
		}
		return ""
	default:
		return ""
	}
}

func overageFromMap(m map[string]interface{}) entities.OverageMetadata {
	o := entities.OverageMetadata{}
	if v, ok := m["enabled"].(bool); ok {
		o.Enabled = v
	}
	if v, ok := m["event_rate"].(float64); ok {
		o.EventRate = v
	}
	if v, ok := m["query_rate"].(float64); ok {
		o.QueryRate = v
	}
	if v, ok := m["events_overage"].(float64); ok {
		o.EventsOverage = int64(v)
	}
	if v, ok := m["queries_overage"].(float64); ok {
		o.QueriesOverage = int64(v)
	}
	if v, ok := m["last_reported_events"].(float64); ok {
		o.LastReportedEvents = int64(v)
	}
	if v, ok := m["last_reported_queries"].(float64); ok {
		o.LastReportedQueries = int64(v)
	}
	return o
}

func quotasFromMap(m map[string]interface{}) entities.QuotaMetadata {
	q := entities.QuotaMetadata{}
	if v, ok := m["events_quota"].(float64); ok {
		q.EventsQuota = int64(v)
	}
	if v, ok := m["queries_quota"].(float64); ok {
		q.QueriesQuota = int64(v)
	}
	if v, ok := m["events_used"].(float64); ok {
		q.EventsUsed = int64(v)
	}
	if v, ok := m["queries_used"].(float64); ok {
		q.QueriesUsed = int64(v)
	}
	if v, ok := m["x402_allowance"].(float64); ok {
		q.X402Allowance = int64(v)
	}
	if v, ok := m["x402_used"].(float64); ok {
		q.X402Used = int64(v)
	}
	if v, ok := m["extraction_tokens_quota"].(float64); ok {
		q.ExtractionTokensQuota = int64(v)
	}
	if v, ok := m["extraction_tokens_used"].(float64); ok {
		q.ExtractionTokensUsed = int64(v)
	}
	if v, ok := m["retention_days"].(float64); ok {
		q.RetentionDays = int64(v)
	}
	if v, ok := m["max_streams"].(float64); ok {
		q.MaxStreams = int64(v)
	}
	if v, ok := m["mcp_scope"].(string); ok {
		q.MCPScope = v
	}
	return q
}
