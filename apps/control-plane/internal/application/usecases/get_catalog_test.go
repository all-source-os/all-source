package usecases

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// catalogMockLS implements clients.LemonSqueezyClient for catalog tests.
// Only LookupVariantID + GetVariant carry behavior; the rest are stubs.
type catalogMockLS struct {
	variants map[string]*clients.VariantResponse // variantID → variant
	getCalls int
	currency string // store currency; "" → USD
	currencyErr bool
}

func (m *catalogMockLS) LookupVariantID(tier, period string) (string, error) {
	if period == "" {
		period = "monthly"
	}
	if period == "yearly" {
		period = "annual"
	}
	id := tier + ":" + period
	if _, ok := m.variants[id]; ok {
		return id, nil
	}
	return "", fmt.Errorf("no variant for %s:%s", tier, period)
}

func (m *catalogMockLS) GetVariant(_ context.Context, variantID string) (*clients.VariantResponse, error) {
	m.getCalls++
	v, ok := m.variants[variantID]
	if !ok {
		return nil, fmt.Errorf("variant %s not found", variantID)
	}
	return v, nil
}

func (m *catalogMockLS) VariantMap() clients.VariantMap { return nil }
func (m *catalogMockLS) GetStoreID() string             { return "store" }
func (m *catalogMockLS) GetStoreCurrency(_ context.Context) (string, error) {
	if m.currencyErr {
		return "", fmt.Errorf("store unavailable")
	}
	if m.currency != "" {
		return m.currency, nil
	}
	return "USD", nil
}

func TestGetCatalog_DoesNotMislabelUnknownCurrency(t *testing.T) {
	ls := &catalogMockLS{
		variants: map[string]*clients.VariantResponse{
			"indie:annual": {Price: 18199, Interval: "year"},
		},
		currencyErr: true,
	}
	cat, err := NewGetCatalogUseCase(ls).Execute(context.Background(), time.Now())
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if len(cat.Tiers) != 0 {
		t.Fatalf("unknown currency must not display prices: %+v", cat.Tiers)
	}
}
func (m *catalogMockLS) UpdateSubscription(_ context.Context, _ string, _ int) (*clients.SubscriptionResponse, error) {
	return nil, fmt.Errorf("not implemented")
}
func (m *catalogMockLS) CreateCheckout(_ context.Context, _ clients.CreateCheckoutRequest) (*clients.CheckoutResponse, error) {
	return nil, fmt.Errorf("not implemented")
}
func (m *catalogMockLS) GetCustomerPortalURL(_ context.Context, _ string) (string, error) {
	return "", fmt.Errorf("not implemented")
}
func (m *catalogMockLS) ReportUsage(_ context.Context, _ clients.ReportUsageRequest) error {
	return fmt.Errorf("not implemented")
}
func (m *catalogMockLS) GetSubscription(_ context.Context, _ string) (*clients.SubscriptionResponse, error) {
	return nil, fmt.Errorf("not implemented")
}
func (m *catalogMockLS) ListSubscriptions(_ context.Context, _ string, _ int) (*clients.SubscriptionListResponse, error) {
	return nil, fmt.Errorf("not implemented")
}
func (m *catalogMockLS) ListInvoices(_ context.Context, _ string, _ int, _ string) (*clients.InvoiceListResponse, error) {
	return nil, fmt.Errorf("not implemented")
}
func (m *catalogMockLS) RefundInvoice(_ context.Context, _ string, _ int) error {
	return fmt.Errorf("not implemented")
}

func TestFormatCents(t *testing.T) {
	cases := []struct {
		cents    int
		currency string
		want     string
	}{
		{1900, "USD", "$19"},
		{1899, "USD", "$18.99"},
		{18199, "GBP", "£181.99"},
		{29899, "GBP", "£298.99"},
		{7899, "GBP", "£78.99"},
		{1900, "EUR", "€19"},
		{0, "GBP", "£0"},
		{1899, "", "$18.99"}, // empty currency → dollars
	}
	for _, c := range cases {
		if got := formatCents(c.cents, c.currency); got != c.want {
			t.Errorf("formatCents(%d, %q) = %q, want %q", c.cents, c.currency, got, c.want)
		}
	}
}

func TestGetCatalog_ReadsLemonSqueezyPrices(t *testing.T) {
	ls := &catalogMockLS{variants: map[string]*clients.VariantResponse{
		"indie:monthly":  {Price: 1899, Interval: "month"},
		"indie:annual":   {Price: 18199, Interval: "year"},
		"studio:monthly": {Price: 7899, Interval: "month"},
		// studio:annual intentionally missing → tier still returned with only monthly
	}}
	uc := NewGetCatalogUseCase(ls)

	cat, err := uc.Execute(context.Background(), time.Unix(1_700_000_000, 0))
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}

	byTier := map[string]CatalogTier{}
	for _, t := range cat.Tiers {
		byTier[t.Tier] = t
	}

	indie, ok := byTier["indie"]
	if !ok {
		t.Fatal("indie missing from catalog")
	}
	if indie.Monthly == nil || indie.Monthly.Formatted != "$18.99" {
		t.Errorf("indie monthly = %+v, want $18.99", indie.Monthly)
	}
	if indie.Annual == nil || indie.Annual.Formatted != "$181.99" {
		t.Errorf("indie annual = %+v, want $181.99", indie.Annual)
	}
	// $181.99/12 = $15.166 → rounds to $15.17 (not truncated $15.16).
	if indie.Annual.PerMonth != "$15.17" {
		t.Errorf("indie annual per-month = %q, want $15.17", indie.Annual.PerMonth)
	}

	studio, ok := byTier["studio"]
	if !ok || studio.Monthly == nil || studio.Monthly.Formatted != "$78.99" {
		t.Errorf("studio monthly = %+v, want $78.99", studio.Monthly)
	}
	if studio.Annual != nil {
		t.Errorf("studio annual should be nil (variant missing), got %+v", studio.Annual)
	}
}

func TestGetCatalog_NilClient_EmptyCatalog(t *testing.T) {
	uc := NewGetCatalogUseCase(nil)
	cat, err := uc.Execute(context.Background(), time.Unix(1_700_000_000, 0))
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if len(cat.Tiers) != 0 {
		t.Errorf("nil LS should yield empty catalog, got %d tiers", len(cat.Tiers))
	}
}

func TestGetCatalog_CachesWithinTTL(t *testing.T) {
	ls := &catalogMockLS{variants: map[string]*clients.VariantResponse{
		"indie:monthly": {Price: 1899, Interval: "month"},
	}}
	uc := NewGetCatalogUseCase(ls)
	base := time.Unix(1_700_000_000, 0)

	if _, err := uc.Execute(context.Background(), base); err != nil {
		t.Fatalf("first Execute: %v", err)
	}
	callsAfterFirst := ls.getCalls
	if callsAfterFirst == 0 {
		t.Fatal("expected LS GetVariant calls on first Execute")
	}
	// Within TTL → served from cache, no new LS calls.
	if _, err := uc.Execute(context.Background(), base.Add(2*time.Minute)); err != nil {
		t.Fatalf("cached Execute: %v", err)
	}
	if ls.getCalls != callsAfterFirst {
		t.Errorf("expected cache hit (no new LS calls); got %d -> %d", callsAfterFirst, ls.getCalls)
	}
	// Past TTL → refetch.
	if _, err := uc.Execute(context.Background(), base.Add(6*time.Minute)); err != nil {
		t.Fatalf("refetch Execute: %v", err)
	}
	if ls.getCalls == callsAfterFirst {
		t.Error("expected LS refetch after TTL expiry")
	}
}

func TestGetCatalog_RejectsWrongBillingInterval(t *testing.T) {
	ls := &catalogMockLS{variants: map[string]*clients.VariantResponse{
		"indie:monthly": {Price: 1899, Interval: "month"},
		"indie:annual":  {Price: 18199, Interval: "month"},
	}}
	cat, err := NewGetCatalogUseCase(ls).Execute(context.Background(), time.Now())
	if err != nil {
		t.Fatalf("Execute: %v", err)
	}
	if len(cat.Tiers) != 1 || cat.Tiers[0].Monthly == nil || cat.Tiers[0].Annual != nil {
		t.Fatalf("wrong-interval annual price must not be displayed: %+v", cat.Tiers)
	}
}
