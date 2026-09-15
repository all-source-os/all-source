package billing

import (
	"context"
	"testing"
	"time"

	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
	"github.com/allsource/control-plane/internal/infrastructure/persistence"
)

var extractionNow = time.Date(2026, 9, 15, 12, 0, 0, 0, time.UTC)

func seedExtractionTenant(t *testing.T, repo *persistence.MemoryTenantRepository, quotas *entities.QuotaMetadata, overage *entities.OverageMetadata) {
	t.Helper()
	if err := repo.Save(&entities.Tenant{
		ID:     "t1",
		Name:   "extraction-tenant",
		Status: entities.TenantStatusActive,
		Metadata: map[string]interface{}{
			"overage":      overage,
			"quotas":       quotas,
			"subscription": &entities.SubscriptionMetadata{Tier: "studio", SubscriptionItemID: "si_1"},
		},
	}); err != nil {
		t.Fatalf("save tenant: %v", err)
	}
}

func newExtractionReporter(repo *persistence.MemoryTenantRepository, ls *mockLSClient, tokensPerUnit int64) *ReportExtractionOverageUseCase {
	uc := NewReportExtractionOverageUseCase(repo, persistence.NewMemoryAuditRepository(), ls, tokensPerUnit)
	uc.now = func() time.Time { return extractionNow }
	return uc
}

func TestReportExtractionOverage_UnsetRateReportsNothing(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	seedExtractionTenant(t, repo,
		&entities.QuotaMetadata{ExtractionTokensQuota: 1000, ExtractionTokensUsed: 50_000},
		&entities.OverageMetadata{Enabled: true})
	ls := &mockLSClient{}

	res, err := newExtractionReporter(repo, ls, 0).Execute(context.Background(), "t1")
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if !res.Skipped || len(ls.reportedUsages) != 0 {
		t.Fatalf("with no rate decided, nothing may be billed: result=%+v reports=%v", res, ls.reportedUsages)
	}
}

func TestReportExtractionOverage_ConvertsTokensToUnitsAndFloors(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	seedExtractionTenant(t, repo,
		&entities.QuotaMetadata{ExtractionTokensQuota: 1000, ExtractionTokensUsed: 3_999},
		&entities.OverageMetadata{Enabled: true})
	ls := &mockLSClient{}

	res, err := newExtractionReporter(repo, ls, 1000).Execute(context.Background(), "t1")
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if res.UnitsReported != 2 || len(ls.reportedUsages) != 1 || ls.reportedUsages[0].Quantity != 2 {
		t.Fatalf("2,999 overage tokens at 1,000/unit is 2 units: result=%+v reports=%v", res, ls.reportedUsages)
	}
	if ls.reportedUsages[0].SubscriptionItemID != "si_1" {
		t.Errorf("reported to %q, want the tenant's subscription item", ls.reportedUsages[0].SubscriptionItemID)
	}
}

func TestReportExtractionOverage_ReportsOnlyTheDeltaWithinAPeriod(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	seedExtractionTenant(t, repo,
		&entities.QuotaMetadata{ExtractionTokensQuota: 1000, ExtractionTokensUsed: 6_000},
		&entities.OverageMetadata{Enabled: true})
	ls := &mockLSClient{}
	uc := newExtractionReporter(repo, ls, 1000)

	if _, err := uc.Execute(context.Background(), "t1"); err != nil {
		t.Fatalf("first Execute: %v", err)
	}
	res, err := uc.Execute(context.Background(), "t1")
	if err != nil {
		t.Fatalf("second Execute: %v", err)
	}
	if !res.Skipped || len(ls.reportedUsages) != 1 {
		t.Fatalf("an unchanged meter must not be billed twice: reports=%v", ls.reportedUsages)
	}
}

func TestReportExtractionOverage_ANewPeriodStartsFromZero(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	seedExtractionTenant(t, repo,
		&entities.QuotaMetadata{ExtractionTokensQuota: 1000, ExtractionTokensUsed: 3_000},
		&entities.OverageMetadata{
			Enabled:                     true,
			LastReportedExtractionUnits: 9,
			ExtractionReportedPeriod:    "2026-08-01T00:00:00Z",
		})
	ls := &mockLSClient{}

	res, err := newExtractionReporter(repo, ls, 1000).Execute(context.Background(), "t1")
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if res.UnitsReported != 2 {
		t.Fatalf("last period's 9 units must not suppress this period's 2: result=%+v", res)
	}
}

func TestReportExtractionOverage_UnlimitedOrDisabledIsNeverBilled(t *testing.T) {
	cases := map[string]struct {
		quotas  *entities.QuotaMetadata
		overage *entities.OverageMetadata
	}{
		"unlimited allowance": {
			quotas:  &entities.QuotaMetadata{ExtractionTokensQuota: -1, ExtractionTokensUsed: 1_000_000},
			overage: &entities.OverageMetadata{Enabled: true},
		},
		"overage disabled": {
			quotas:  &entities.QuotaMetadata{ExtractionTokensQuota: 1000, ExtractionTokensUsed: 1_000_000},
			overage: &entities.OverageMetadata{Enabled: false},
		},
	}
	for name, tc := range cases {
		t.Run(name, func(t *testing.T) {
			repo := persistence.NewMemoryTenantRepository()
			seedExtractionTenant(t, repo, tc.quotas, tc.overage)
			ls := &mockLSClient{}
			if _, err := newExtractionReporter(repo, ls, 1000).Execute(context.Background(), "t1"); err != nil {
				t.Fatalf("Execute: %v", err)
			}
			if len(ls.reportedUsages) != 0 {
				t.Fatalf("billed %v", ls.reportedUsages)
			}
		})
	}
}

// The metering pipeline end to end: prime.extraction.usage events in Core are
// reconciled into the meter, and the reporter bills what exceeds the allowance.
func TestExtractionBilling_SyncThenReport(t *testing.T) {
	repo := persistence.NewMemoryTenantRepository()
	seedExtractionTenant(t, repo,
		&entities.QuotaMetadata{ExtractionTokensQuota: 1000},
		&entities.OverageMetadata{Enabled: true})
	core := &extractionMockCore{events: []clients.EventEntry{usageEvent(2_500), usageEvent(1_700)}}
	ls := &mockLSClient{}

	if _, err := NewSyncExtractionUsageUseCase(repo, persistence.NewMemoryAuditRepository(), core).Execute(context.Background(), "t1"); err != nil {
		t.Fatalf("sync: %v", err)
	}
	results := newExtractionReporter(repo, ls, 1000).ExecuteAll(context.Background())

	if len(results) != 1 || results[0].Error != nil {
		t.Fatalf("results: %+v", results)
	}
	if len(ls.reportedUsages) != 1 || ls.reportedUsages[0].Quantity != 3 {
		t.Fatalf("4,200 tokens used against 1,000 included at 1,000/unit is 3 units: reports=%v", ls.reportedUsages)
	}
}
