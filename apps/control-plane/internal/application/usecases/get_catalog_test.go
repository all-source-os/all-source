package usecases

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// catalogMockLS implements clients.LemonSqueezyClient for catalog tests.
// Only LookupVariantID + GetVariant carry behavior; the rest are stubs.
type catalogMockLS struct {
	variants    map[string]*clients.VariantResponse // variantID → variant
	getCalls    int
	currency    string // store currency; "" → USD
	currencyErr bool
	variantMap  clients.VariantMap
	getGate     chan struct{}
	getStarted  chan struct{}
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
	if m.getGate != nil {
		select {
		case m.getStarted <- struct{}{}:
		default:
		}
		<-m.getGate
	}
	m.getCalls++
	v, ok := m.variants[variantID]
	if !ok {
		return nil, fmt.Errorf("variant %s not found", variantID)
	}
	return v, nil
}

func (m *catalogMockLS) VariantMap() clients.VariantMap { return m.variantMap }
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
	ls := fullCatalogMockLS()
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
	// Past TTL → stale price returns immediately while detached refresh runs.
	if _, err := uc.Execute(context.Background(), base.Add(6*time.Minute)); err != nil {
		t.Fatalf("refetch Execute: %v", err)
	}
	uc.mu.Lock()
	wait := uc.refresh
	uc.mu.Unlock()
	if wait != nil {
		<-wait
	}
	if ls.getCalls == callsAfterFirst {
		t.Error("expected LS refetch after TTL expiry")
	}
}

type catalogMockStore struct {
	mu     sync.Mutex
	entry  *clients.ConfigEntryResponse
	writes int
}

func (s *catalogMockStore) GetConfig(_ context.Context, _ string) (*clients.ConfigEntryResponse, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.entry, nil
}

func (s *catalogMockStore) SetConfig(_ context.Context, req clients.SetConfigRequest) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.entry = &clients.ConfigEntryResponse{Key: req.Key, Value: req.Value}
	s.writes++
	return nil
}

func fullCatalogMockLS() *catalogMockLS {
	return &catalogMockLS{variants: map[string]*clients.VariantResponse{
		"indie:monthly":  {Price: 1899, Interval: "month"},
		"indie:annual":   {Price: 18199, Interval: "year"},
		"studio:monthly": {Price: 7899, Interval: "month"},
		"studio:annual":  {Price: 75799, Interval: "year"},
		"scale:monthly":  {Price: 29899, Interval: "month"},
		"scale:annual":   {Price: 286999, Interval: "year"},
	}}
}

func TestGetCatalog_CanceledRequestCannotPoisonCache(t *testing.T) {
	ls := fullCatalogMockLS()
	ls.getGate = make(chan struct{})
	ls.getStarted = make(chan struct{}, 1)
	uc := NewGetCatalogUseCase(ls)
	ctx, cancel := context.WithCancel(context.Background())
	result := make(chan *Catalog, 1)
	//nolint:errcheck // the test cancels ctx, so the error is expected; the
	// assertion is on the catalog the caller still gets back.
	go func() { cat, _ := uc.Execute(ctx, time.Now()); result <- cat }()
	<-ls.getStarted
	cancel()
	if cat := <-result; len(cat.Tiers) != 0 {
		t.Fatalf("canceled first request should be empty, got %+v", cat.Tiers)
	}
	close(ls.getGate)
	uc.mu.Lock()
	wait := uc.refresh
	uc.mu.Unlock()
	if wait != nil {
		<-wait
	}
	cat, err := uc.Execute(context.Background(), time.Now())
	if err != nil || !catalogComplete(cat) {
		t.Fatalf("detached refresh should fill cache: %+v, %v", cat, err)
	}
}

func TestGetCatalog_PersistsTwoWeekLastKnownGood(t *testing.T) {
	base := time.Now().UTC().Truncate(time.Second)
	store := &catalogMockStore{}
	first, err := NewGetCatalogUseCase(fullCatalogMockLS(), store).Execute(context.Background(), base)
	if err != nil || !catalogComplete(first) || store.writes != 1 {
		t.Fatalf("first provider read must persist complete catalog: %+v, writes=%d, err=%v", first, store.writes, err)
	}

	offline := fullCatalogMockLS()
	offline.currencyErr = true
	second, err := NewGetCatalogUseCase(offline, store).Execute(context.Background(), base.Add(13*24*time.Hour))
	if err != nil || !catalogComplete(second) || !second.Stale || second.FetchedAt == "" {
		t.Fatalf("cached prices must survive restart and provider outage: %+v, %v", second, err)
	}

	third, err := NewGetCatalogUseCase(offline, store).Execute(context.Background(), base.Add(15*24*time.Hour))
	if err != nil || len(third.Tiers) != 0 {
		t.Fatalf("prices older than two weeks must not be displayed: %+v, %v", third, err)
	}
}

func TestGetCatalog_RejectsChangedVariantMap(t *testing.T) {
	base := time.Now().UTC().Truncate(time.Second)
	store := &catalogMockStore{}
	_, _ = NewGetCatalogUseCase(fullCatalogMockLS(), store).Execute(context.Background(), base) //nolint:errcheck // seeds the cache; the assertion is on later state
	changed := fullCatalogMockLS()
	changed.currencyErr = true
	changed.variantMap = clients.VariantMap{"indie:monthly": "replacement"}
	cat, err := NewGetCatalogUseCase(changed, store).Execute(context.Background(), base.Add(time.Hour))
	if err != nil || len(cat.Tiers) != 0 {
		t.Fatalf("different variant map must invalidate stored prices: %+v, %v", cat, err)
	}
}

func waitForCatalogRefresh(uc *GetCatalogUseCase) {
	uc.mu.Lock()
	wait := uc.refresh
	uc.mu.Unlock()
	if wait != nil {
		<-wait
	}
}

func TestGetCatalog_PriceChangePersistsWithoutWaitingADay(t *testing.T) {
	base := time.Now().UTC().Truncate(time.Second)
	store := &catalogMockStore{}
	ls := fullCatalogMockLS()
	uc := NewGetCatalogUseCase(ls, store)
	_, _ = uc.Execute(context.Background(), base) //nolint:errcheck // seeds the cache; the assertion is on later state
	ls.variants["indie:monthly"] = &clients.VariantResponse{Price: 2199, Interval: "month"}
	_, _ = uc.Execute(context.Background(), base.Add(6*time.Minute)) //nolint:errcheck // seeds the cache; the assertion is on later state
	waitForCatalogRefresh(uc)
	if store.writes != 2 {
		t.Fatalf("changed provider price must persist immediately; writes=%d", store.writes)
	}
	restarted := NewGetCatalogUseCase(fullCatalogMockLS(), store)
	cat, _ := restarted.Execute(context.Background(), base.Add(7*time.Minute)) //nolint:errcheck // asserts the restarted instance's catalog, not its error
	if cat.Tiers[0].Monthly.Formatted != "$21.99" {
		t.Fatalf("restart must load changed provider price, got %+v", cat.Tiers[0].Monthly)
	}
}

func TestGetCatalog_PartialRefreshKeepsLastCompleteSnapshot(t *testing.T) {
	base := time.Now().UTC().Truncate(time.Second)
	store := &catalogMockStore{}
	ls := fullCatalogMockLS()
	uc := NewGetCatalogUseCase(ls, store)
	_, _ = uc.Execute(context.Background(), base) //nolint:errcheck // seeds the cache; the assertion is on later state
	delete(ls.variants, "scale:annual")
	_, _ = uc.Execute(context.Background(), base.Add(6*time.Minute)) //nolint:errcheck // seeds the cache; the assertion is on later state
	waitForCatalogRefresh(uc)
	cat, _ := uc.Execute(context.Background(), base.Add(6*time.Minute+time.Second)) //nolint:errcheck // asserts the served catalog, not the error
	if !catalogComplete(cat) || store.writes != 1 {
		t.Fatalf("partial provider result must not overwrite full catalog: %+v, writes=%d", cat, store.writes)
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
