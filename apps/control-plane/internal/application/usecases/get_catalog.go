package usecases

import (
	"context"
	"fmt"
	"log"
	"strings"
	"sync"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// catalogTiers are the paid, self-serve tiers whose prices come from
// LemonSqueezy. Self-Host (Free) and Enterprise (Custom) have no LS variant, so
// the frontend renders those from static config — they are not in the catalog.
var catalogTiers = []string{"indie", "studio", "scale"}

const (
	catalogTTL          = 5 * time.Minute
	catalogRetryDelay   = 30 * time.Second
	catalogMaxStale     = 14 * 24 * time.Hour
	catalogPersistEvery = 24 * time.Hour
	catalogFetchTimeout = 45 * time.Second
	catalogStoreKey     = "billing:lemon_squeezy:catalog:v1"
)

// CatalogPrice is one tier+period price, sourced from LemonSqueezy.
type CatalogPrice struct {
	Cents     int    `json:"cents"`               // monthly: per-month; annual: total/yr
	Formatted string `json:"formatted"`           // e.g. "$18.99" (or "$181.99" for an annual total)
	PerMonth  string `json:"per_month,omitempty"` // annual only: the per-month equivalent, e.g. "$15.17"
}

// CatalogTier is the price pair for a single tier.
type CatalogTier struct {
	Tier    string        `json:"tier"`
	Monthly *CatalogPrice `json:"monthly,omitempty"`
	Annual  *CatalogPrice `json:"annual,omitempty"`
}

// Catalog is the public pricing catalog, read live from LemonSqueezy.
type Catalog struct {
	Currency  string        `json:"currency"`
	Tiers     []CatalogTier `json:"tiers"`
	FetchedAt string        `json:"fetched_at,omitempty"`
	Stale     bool          `json:"stale,omitempty"`
}

// GetCatalogUseCase serves a two-week last-known-good Lemon Squeezy snapshot.
// Refresh runs beyond an HTTP request deadline; empty results never replace it.
type GetCatalogUseCase struct {
	ls    clients.LemonSqueezyClient
	store CatalogStore

	loadOnce sync.Once
	mu       sync.Mutex
	cached   *Catalog
	cachedAt time.Time
	complete bool
	savedAt  time.Time
	nextTry  time.Time
	refresh  chan struct{}
}

// store is optional for tests and installations without persistent Core config.
func NewGetCatalogUseCase(ls clients.LemonSqueezyClient, store ...CatalogStore) *GetCatalogUseCase {
	uc := &GetCatalogUseCase{ls: ls}
	if len(store) > 0 {
		uc.store = store[0]
	}
	return uc
}

// currencySymbol maps an ISO currency code to its display symbol; unknown codes
// fall back to a "CODE " prefix (e.g. "SEK 189") so the amount is never silently
// mislabeled as dollars.
func currencySymbol(code string) string {
	switch strings.ToUpper(code) {
	case "USD", "AUD", "CAD", "NZD", "":
		return "$"
	case "GBP":
		return "£"
	case "EUR":
		return "€"
	case "JPY":
		return "¥"
	default:
		return strings.ToUpper(code) + " "
	}
}

// formatCents renders cents as a price string in the given currency: whole units
// drop the decimals ("£19"), otherwise two decimals ("£18.99").
func formatCents(cents int, currency string) string {
	sym := currencySymbol(currency)
	if cents%100 == 0 {
		return fmt.Sprintf("%s%d", sym, cents/100)
	}
	return fmt.Sprintf("%s%.2f", sym, float64(cents)/100)
}

// Execute serves cached prices immediately. On a cold cache it waits for one
// detached refresh until the caller's deadline; that refresh continues if the
// browser disconnects, ready for the next request.
func (uc *GetCatalogUseCase) Execute(ctx context.Context, now time.Time) (*Catalog, error) {
	if uc.ls == nil {
		return &Catalog{Tiers: []CatalogTier{}}, nil
	}
	uc.loadOnce.Do(func() {
		loadCtx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		uc.loadSnapshot(loadCtx, now)
	})

	uc.mu.Lock()
	maxAge := catalogMaxStale
	if !uc.complete {
		maxAge = catalogRetryDelay
	}
	if uc.cached != nil && now.Sub(uc.cachedAt) > maxAge {
		uc.cached = nil
	}
	if uc.cached != nil && now.Sub(uc.cachedAt) < catalogTTL {
		c := uc.response(now)
		uc.mu.Unlock()
		return c, nil
	}
	if uc.refresh == nil && !now.Before(uc.nextTry) {
		uc.refresh = make(chan struct{})
		go uc.refreshCatalog(now)
	}
	if uc.cached != nil {
		c := uc.response(now)
		uc.mu.Unlock()
		return c, nil
	}
	wait := uc.refresh
	uc.mu.Unlock()

	if wait != nil {
		select {
		case <-wait:
		case <-ctx.Done():
		}
	}
	uc.mu.Lock()
	defer uc.mu.Unlock()
	if uc.cached != nil && now.Sub(uc.cachedAt) <= maxAge {
		return uc.response(now), nil
	}
	return &Catalog{Tiers: []CatalogTier{}}, nil
}

// response copies response metadata; cached price tiers remain immutable.
func (uc *GetCatalogUseCase) response(now time.Time) *Catalog {
	cat := *uc.cached
	cat.FetchedAt = uc.cachedAt.UTC().Format(time.RFC3339)
	cat.Stale = now.Sub(uc.cachedAt) >= 24*time.Hour
	return &cat
}

// fetchProvider never populates a cache itself. Its caller decides whether a
// response is complete enough to become a durable last-known-good snapshot.
func (uc *GetCatalogUseCase) fetchProvider(ctx context.Context) *Catalog {
	cat := &Catalog{Tiers: make([]CatalogTier, 0, len(catalogTiers))}
	// Never label an unknown store currency as USD; that could misstate the
	// amount customers will see at checkout.
	currency, err := uc.ls.GetStoreCurrency(ctx)
	if err != nil || currency == "" {
		if err != nil {
			log.Printf("billing catalog currency lookup failed: %v", err)
		}
		return nil
	}
	cat.Currency = strings.ToUpper(currency)

	for _, tier := range catalogTiers {
		entry := CatalogTier{Tier: tier}
		if p := uc.price(ctx, tier, defaultBillingPeriod, false, cat.Currency); p != nil {
			entry.Monthly = p
		}
		if p := uc.price(ctx, tier, annualBillingPeriod, true, cat.Currency); p != nil {
			entry.Annual = p
		}
		// Only include a tier if at least one period resolved.
		if entry.Monthly != nil || entry.Annual != nil {
			cat.Tiers = append(cat.Tiers, entry)
		}
	}

	return cat
}

// price resolves one tier+period to a CatalogPrice. Returns nil (skipped, not
// fatal) when the variant isn't configured or LS lookup fails, so one missing
// price never blanks the whole catalog. For annual, also computes the per-month
// equivalent from the annual total.
func (uc *GetCatalogUseCase) price(ctx context.Context, tier, period string, annual bool, currency string) *CatalogPrice {
	variantID, err := uc.ls.LookupVariantID(tier, period)
	if err != nil || variantID == "" {
		log.Printf("billing catalog variant missing: tier=%s period=%s", tier, period)
		return nil
	}
	v, err := uc.ls.GetVariant(ctx, variantID)
	expectedInterval := "month"
	if annual {
		expectedInterval = "year"
	}
	if err != nil || v == nil || v.Price <= 0 || v.Interval != expectedInterval {
		log.Printf("billing catalog price unresolved: tier=%s period=%s", tier, period)
		return nil
	}
	p := &CatalogPrice{Cents: v.Price, Formatted: formatCents(v.Price, currency)}
	if annual {
		// Round to nearest cent (half-up) rather than truncate: 18199/12 is
		// 1516.6 → "£15.17", not "£15.16".
		p.PerMonth = formatCents((v.Price+6)/12, currency)
	}
	return p
}
