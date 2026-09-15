package usecases

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"log"
	"reflect"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// CatalogStore is Core's existing persistent config store. It holds only public
// list prices, source identity, and last provider-check time; no new Fly volume.
type CatalogStore interface {
	GetConfig(context.Context, string) (*clients.ConfigEntryResponse, error)
	SetConfig(context.Context, clients.SetConfigRequest) error
}

type catalogSnapshot struct {
	Fingerprint string    `json:"fingerprint"`
	FetchedAt   time.Time `json:"fetched_at"`
	Catalog     Catalog   `json:"catalog"`
}

func (uc *GetCatalogUseCase) loadSnapshot(ctx context.Context, now time.Time) {
	if uc.store == nil {
		return
	}
	entry, err := uc.store.GetConfig(ctx, catalogStoreKey)
	if err != nil || entry == nil {
		return // missing entry is normal on first deployment
	}
	raw, ok := entry.Value.(string)
	if !ok {
		return
	}
	var snap catalogSnapshot
	if json.Unmarshal([]byte(raw), &snap) != nil ||
		snap.Fingerprint != uc.fingerprint() ||
		snap.FetchedAt.IsZero() || snap.FetchedAt.After(now.Add(5*time.Minute)) ||
		now.Sub(snap.FetchedAt) > catalogMaxStale || !catalogComplete(&snap.Catalog) {
		return
	}
	uc.mu.Lock()
	uc.cached = &snap.Catalog
	uc.cachedAt = snap.FetchedAt
	uc.complete = true
	uc.savedAt = snap.FetchedAt
	uc.mu.Unlock()
}

// A changed store or variant map invalidates old prices, even before expiry.
func (uc *GetCatalogUseCase) fingerprint() string {
	mapJSON, err := json.Marshal(uc.ls.VariantMap()) // encoding/json sorts map keys
	if err != nil {
		// Falling back to a constant would collapse every variant map onto one
		// fingerprint and serve a stale catalog after a store change. %v also
		// sorts map keys, so the fallback stays deterministic.
		mapJSON = []byte(fmt.Sprintf("%v", uc.ls.VariantMap()))
	}
	sum := sha256.Sum256(append([]byte(uc.ls.GetStoreID()+":"), mapJSON...))
	return fmt.Sprintf("%x", sum)
}

// Only a fully resolved six-price catalog can replace a previous snapshot.
// Partial provider failures may show available prices on a cold cache, but
// cannot erase a complete last-known-good catalog or persist incomplete data.
func catalogComplete(cat *Catalog) bool {
	if cat == nil || cat.Currency == "" || len(cat.Tiers) != len(catalogTiers) {
		return false
	}
	seen := make(map[string]bool, len(cat.Tiers))
	for _, tier := range cat.Tiers {
		if seen[tier.Tier] || tier.Monthly == nil || tier.Annual == nil ||
			tier.Monthly.Cents <= 0 || tier.Annual.Cents <= 0 ||
			tier.Monthly.Formatted != formatCents(tier.Monthly.Cents, cat.Currency) ||
			tier.Annual.Formatted != formatCents(tier.Annual.Cents, cat.Currency) ||
			tier.Annual.PerMonth != formatCents((tier.Annual.Cents+6)/12, cat.Currency) {
			return false
		}
		seen[tier.Tier] = true
	}
	for _, tier := range catalogTiers {
		if !seen[tier] {
			return false
		}
	}
	return true
}

func (uc *GetCatalogUseCase) refreshCatalog(now time.Time) {
	// Detach from request cancellation: short web deadlines cannot poison a
	// five-minute cache with an empty result from a half-fetched catalog.
	ctx, cancel := context.WithTimeout(context.Background(), catalogFetchTimeout)
	defer cancel()
	cat := uc.fetchProvider(ctx)
	complete := catalogComplete(cat)

	uc.mu.Lock()
	changed := complete && (uc.cached == nil || !reflect.DeepEqual(*uc.cached, *cat))
	if cat != nil && len(cat.Tiers) > 0 && (uc.cached == nil || complete) {
		uc.cached = cat
		uc.cachedAt = now
		uc.complete = complete
	}
	switch {
	case complete, uc.cached != nil && uc.complete:
		uc.nextTry = now.Add(catalogTTL)
	default:
		uc.nextTry = now.Add(catalogRetryDelay)
	}
	servingLastGood := uc.cached != nil && uc.complete
	save := complete && uc.store != nil && (changed || now.Sub(uc.savedAt) >= catalogPersistEvery)
	uc.mu.Unlock()
	if !complete {
		resolved := 0
		if cat != nil {
			for _, tier := range cat.Tiers {
				if tier.Monthly != nil {
					resolved++
				}
				if tier.Annual != nil {
					resolved++
				}
			}
		}
		log.Printf("billing catalog refresh incomplete: resolved=%d/6 last_good=%t", resolved, servingLastGood)
	}

	if save {
		snap := catalogSnapshot{Fingerprint: uc.fingerprint(), FetchedAt: now, Catalog: *cat}
		data, err := json.Marshal(snap)
		if err == nil {
			err = uc.store.SetConfig(ctx, clients.SetConfigRequest{
				Key: catalogStoreKey, Value: string(data), ChangedBy: "billing-catalog",
			})
		}
		if err != nil {
			log.Printf("billing catalog: failed to persist last-known-good prices: %v", err)
		} else {
			uc.mu.Lock()
			uc.savedAt = now
			uc.mu.Unlock()
			log.Printf("billing catalog: persisted complete Lemon Squeezy snapshot at %s", now.UTC().Format(time.RFC3339))
		}
	}

	uc.mu.Lock()
	close(uc.refresh)
	uc.refresh = nil
	uc.mu.Unlock()
}
