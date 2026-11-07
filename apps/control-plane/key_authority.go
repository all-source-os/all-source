package main

import (
	"context"
	"errors"
	"fmt"
	"log"
	"strings"
	"sync"
	"time"

	"github.com/prometheus/client_golang/prometheus"
)

type keyAuthorityMode string

const (
	keyAuthorityOff     keyAuthorityMode = "off"
	keyAuthorityShadow  keyAuthorityMode = "shadow"
	keyAuthorityEnforce keyAuthorityMode = "enforce"

	keyAuthorityModeEnv = "ALLSOURCE_KEY_AUTHORITY_MODE"

	keyAuthorityCacheTTL   = 30 * time.Second
	keyAuthorityCacheLimit = 10_000
	// Shadow reuses an unavailable answer for this long, so Core shedding load
	// gets one read per key, not one per request.
	keyAuthorityUnavailableBackoff = 5 * time.Second
	denialLogInterval              = time.Minute

	denialRevoked      = "revoked"
	denialUnregistered = "unregistered"
	denialExpired      = "expired"
	denialHistory      = "history_invalid"
	denialIncomplete   = "incomplete_authority"
	denialUnavailable  = "authority_unavailable"
	denialScope        = "scope"
	denialKind         = "credential_kind"
	denialOther        = "other"

	denialAssertedSignature    = "asserted_signature"
	denialAssertedAudience     = "asserted_audience"
	denialAssertedSkew         = "asserted_skew"
	denialAssertedExpired      = "asserted_expired"
	denialAssertedUnregistered = "asserted_unregistered"
	denialAssertedRevoked      = "asserted_revoked"
	denialAssertedUnavailable  = "asserted_unavailable"
)

var errKeyAuthorityUnavailable = errors.New("API key authority unavailable")

// Only a key with an apikey.created record can be revoked; a revocation naming
// an unregistered id leaves the key unregistered. Shadow refuses a revoked key
// as enforce does, so it can never refuse a legacy onboarding key.
var errRevokedAPIKey = errors.New("revoked API key")

// errStrayRevokedAPIKey still counts as unregistered, but the legacy sunset
// never serves it, so enforce refuses a key a tenant revoked.
var errStrayRevokedAPIKey = errors.New("unregistered API key named by a revocation")

// Counts every request the managed-key authority refuses, or would refuse in
// shadow mode, so a credential policy change is visible per tenant before it
// is enforced. Tenant comes from the signed claims, never "anonymous".
var keyAuthorityDenials = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_auth_denials_total",
	Help: "API-key requests refused by the managed-key authority, or that would be refused in shadow mode.",
}, []string{"mode", "reason", "tenant"})

func parseKeyAuthorityMode(raw string) (keyAuthorityMode, error) {
	mode := keyAuthorityMode(strings.ToLower(strings.TrimSpace(raw)))
	switch mode {
	case "":
		return keyAuthorityShadow, nil
	case keyAuthorityOff, keyAuthorityShadow, keyAuthorityEnforce:
		return mode, nil
	}
	return "", fmt.Errorf("%s must be off, shadow or enforce, got %q", keyAuthorityModeEnv, raw)
}

func denialReason(err error) string {
	switch msg := err.Error(); {
	case errors.Is(err, errRevokedAPIKey):
		return denialRevoked
	case errors.Is(err, errKeyAuthorityUnavailable), strings.Contains(msg, "authority unavailable"):
		return denialUnavailable
	case strings.Contains(msg, "incomplete API key authority"):
		return denialIncomplete
	case strings.Contains(msg, "invalid API key history"):
		return denialHistory
	case strings.Contains(msg, "expired"), strings.Contains(msg, "expiration contract"):
		return denialExpired
	case strings.Contains(msg, "unregistered"):
		return denialUnregistered
	}
	return denialOther
}

type keyLookup struct {
	record  *managedKeyRecord
	err     error
	expires time.Time
}

// keyAuthority wraps the per-request Core lookup with a short cache, so the
// authority costs Core one read per key per TTL instead of one per request.
type keyAuthority struct {
	mode   keyAuthorityMode
	lookup func(context.Context, *Claims) (*managedKeyRecord, error)
	now    func() time.Time

	mu         sync.Mutex
	cache      map[string]keyLookup
	lastLogged map[string]time.Time
}

func newKeyAuthority(mode keyAuthorityMode, lookup func(context.Context, *Claims) (*managedKeyRecord, error)) *keyAuthority {
	return &keyAuthority{mode: mode, lookup: lookup, now: time.Now, cache: map[string]keyLookup{}, lastLogged: map[string]time.Time{}}
}

// Under enforce every request reads the authority, so a revocation takes effect
// on the next request; the cache only protects Core while shadowing.
func (k *keyAuthority) resolve(ctx context.Context, claims *Claims) (*managedKeyRecord, error) {
	if k.mode == keyAuthorityEnforce {
		return k.lookup(ctx, claims)
	}
	cacheKey := claims.TenantID + "\x00" + claims.UserID + "\x00" + fmt.Sprint(claims.IssuedAt)
	now := k.now()
	k.mu.Lock()
	if hit, ok := k.cache[cacheKey]; ok && now.Before(hit.expires) {
		k.mu.Unlock()
		return hit.record, hit.err
	}
	k.mu.Unlock()

	record, err := k.lookup(ctx, claims)
	ttl := keyAuthorityCacheTTL
	if err != nil && denialReason(err) == denialUnavailable {
		// An outage is not an answer about the key: hold it only for the back-off.
		ttl = keyAuthorityUnavailableBackoff
	}
	k.mu.Lock()
	if len(k.cache) >= keyAuthorityCacheLimit {
		k.cache = map[string]keyLookup{}
	}
	k.cache[cacheKey] = keyLookup{record: record, err: err, expires: now.Add(ttl)}
	k.mu.Unlock()
	return record, err
}

func (k *keyAuthority) record(tenant, reason string, err error) {
	k.recordAs(k.mode, tenant, reason, err)
}

func (k *keyAuthority) recordAs(mode keyAuthorityMode, tenant, reason string, err error) {
	keyAuthorityDenials.WithLabelValues(string(mode), reason, tenantLabel(tenant)).Inc()
	logKey := string(mode) + "\x00" + tenant + "\x00" + reason
	now := k.now()
	k.mu.Lock()
	last, seen := k.lastLogged[logKey]
	if seen && now.Sub(last) < denialLogInterval {
		k.mu.Unlock()
		return
	}
	k.lastLogged[logKey] = now
	k.mu.Unlock()
	verb := "would refuse"
	if mode == keyAuthorityEnforce || reason == denialRevoked {
		verb = "refused"
	}
	detail := ""
	if err != nil {
		detail = err.Error()
	}
	log.Printf("API key authority %s: mode=%s tenant=%s reason=%s detail=%q", verb, mode, tenant, reason, detail) //nolint:gosec // G706: tenant comes from a verified signature or is "unverified", reason from a fixed set.
}
