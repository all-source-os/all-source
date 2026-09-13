package usecases

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// catalogTiers are the paid, self-serve tiers whose prices come from
// LemonSqueezy. Self-Host (Free) and Enterprise (Custom) have no LS variant, so
// the frontend renders those from static config — they are not in the catalog.
var catalogTiers = []string{"indie", "studio", "scale"}

const catalogTTL = 5 * time.Minute

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
	Currency string        `json:"currency"`
	Tiers    []CatalogTier `json:"tiers"`
}

// GetCatalogUseCase serves the pricing catalog from LemonSqueezy (the source of
// truth for what customers are actually charged), with a short in-memory TTL
// cache so a price page render doesn't fan out to LS on every request.
type GetCatalogUseCase struct {
	ls clients.LemonSqueezyClient

	mu       sync.Mutex
	cached   *Catalog
	cachedAt time.Time
}

// NewGetCatalogUseCase creates the catalog use case. ls may be nil (no LS
// configured) — Execute then returns an empty catalog and the frontend hides
// paid prices until the Lemon Squeezy catalog is available.
func NewGetCatalogUseCase(ls clients.LemonSqueezyClient) *GetCatalogUseCase {
	return &GetCatalogUseCase{ls: ls}
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

// Execute returns the catalog, using the TTL cache when warm. `now` is injected
// for testability; callers pass time.Now().
func (uc *GetCatalogUseCase) Execute(ctx context.Context, now time.Time) (*Catalog, error) {
	uc.mu.Lock()
	if uc.cached != nil && now.Sub(uc.cachedAt) < catalogTTL {
		c := uc.cached
		uc.mu.Unlock()
		return c, nil
	}
	uc.mu.Unlock()

	cat := &Catalog{Tiers: make([]CatalogTier, 0, len(catalogTiers))}
	if uc.ls == nil {
		return cat, nil
	}

	// Never label an unknown store currency as USD; that could misstate the
	// amount customers will see at checkout.
	currency, err := uc.ls.GetStoreCurrency(ctx)
	if err != nil || currency == "" {
		return cat, nil
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

	uc.mu.Lock()
	uc.cached = cat
	uc.cachedAt = now
	uc.mu.Unlock()
	return cat, nil
}

// price resolves one tier+period to a CatalogPrice. Returns nil (skipped, not
// fatal) when the variant isn't configured or LS lookup fails, so one missing
// price never blanks the whole catalog. For annual, also computes the per-month
// equivalent from the annual total.
func (uc *GetCatalogUseCase) price(ctx context.Context, tier, period string, annual bool, currency string) *CatalogPrice {
	variantID, err := uc.ls.LookupVariantID(tier, period)
	if err != nil || variantID == "" {
		return nil
	}
	v, err := uc.ls.GetVariant(ctx, variantID)
	expectedInterval := "month"
	if annual {
		expectedInterval = "year"
	}
	if err != nil || v == nil || v.Price <= 0 || v.Interval != expectedInterval {
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
