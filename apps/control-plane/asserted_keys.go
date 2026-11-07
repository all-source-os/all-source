package main

import (
	"bytes"
	"context"
	"crypto/ecdh"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"math/big"
	"net/http"
	"net/url"
	"os"
	"slices"
	"strings"
	"sync"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/corehold"
)

const (
	clientKeyEntityPrefix = "clientkey:"
	clientKeyEventPrefix  = "clientkey."
	clientKeyRegistered   = "clientkey.registered"
	clientKeyRevoked      = "clientkey.revoked"
	clientAssertionKind   = "client_assertion"
	assertionAlg          = "ES256"

	assertionAudienceEnv     = "ALLSOURCE_ASSERTION_AUDIENCE"
	defaultAssertionAudience = "https://api.all-source.xyz"

	assertionMaxLifetime = 60 * time.Second
	assertionClockSkew   = 30 * time.Second

	// A revocation is observed within this long on every gateway machine.
	assertedKeyCacheTTL = 30 * time.Second
	// While Core cannot answer, a key read successfully within this window is
	// still honored; a revoked one never is.
	assertedKeyStaleLimit  = 5 * time.Minute
	assertedKeyCacheLimit  = 10_000
	assertedKeyLookupRate  = 20.0
	assertedKeyLookupBurst = 40.0
)

// validKid accepts only the shape of an RFC 7638 SHA-256 thumbprint.
func validKid(kid string) bool {
	if len(kid) != 43 {
		return false
	}
	raw, err := strictBase64URL.DecodeString(kid)
	return err == nil && len(raw) == sha256.Size
}

func assertionAudienceFromEnv() string {
	if audience := strings.TrimSpace(os.Getenv(assertionAudienceEnv)); audience != "" {
		return audience
	}
	return defaultAssertionAudience
}

// ecJWK is an RFC 7517 P-256 key. D is set only on a canary's private key.
type ecJWK struct {
	Kty string `json:"kty"`
	Crv string `json:"crv"`
	X   string `json:"x"`
	Y   string `json:"y"`
	D   string `json:"d,omitempty"`
}

var strictBase64URL = base64.RawURLEncoding.Strict()

func jwkCoordinate(value string) ([]byte, error) {
	raw, err := strictBase64URL.DecodeString(value)
	if err != nil || len(raw) != 32 {
		return nil, errors.New("JWK coordinates must be 32 bytes of unpadded base64url")
	}
	return raw, nil
}

// point returns the uncompressed SEC 1 encoding after checking it lies on P-256.
func (j *ecJWK) point() ([]byte, error) {
	if j.Kty != "EC" || j.Crv != "P-256" {
		return nil, errors.New("JWK must be kty EC with crv P-256")
	}
	x, err := jwkCoordinate(j.X)
	if err != nil {
		return nil, err
	}
	y, err := jwkCoordinate(j.Y)
	if err != nil {
		return nil, err
	}
	encoded := make([]byte, 0, 65)
	encoded = append(encoded, 4)
	encoded = append(encoded, x...)
	encoded = append(encoded, y...)
	if _, err := ecdh.P256().NewPublicKey(encoded); err != nil {
		return nil, errors.New("JWK point is not on P-256")
	}
	return encoded, nil
}

func (j *ecJWK) publicKey() (*ecdsa.PublicKey, error) {
	encoded, err := j.point()
	if err != nil {
		return nil, err
	}
	return &ecdsa.PublicKey{Curve: elliptic.P256(), X: new(big.Int).SetBytes(encoded[1:33]), Y: new(big.Int).SetBytes(encoded[33:])}, nil
}

func (j *ecJWK) privateKey() (*ecdsa.PrivateKey, error) {
	encoded, err := j.point()
	if err != nil {
		return nil, err
	}
	d, err := jwkCoordinate(j.D)
	if err != nil {
		return nil, errors.New("private JWK needs a 32-byte d")
	}
	private, err := ecdh.P256().NewPrivateKey(d)
	if err != nil || !bytes.Equal(private.PublicKey().Bytes(), encoded) {
		return nil, errors.New("private JWK d does not match x and y")
	}
	return &ecdsa.PrivateKey{PublicKey: ecdsa.PublicKey{Curve: elliptic.P256(), X: new(big.Int).SetBytes(encoded[1:33]), Y: new(big.Int).SetBytes(encoded[33:])}, D: new(big.Int).SetBytes(d)}, nil
}

// thumbprint is the RFC 7638 JWK thumbprint. Callers validate X and Y first,
// so neither can carry a character JSON would escape.
func (j *ecJWK) thumbprint() string {
	sum := sha256.Sum256([]byte(`{"crv":"P-256","kty":"EC","x":"` + j.X + `","y":"` + j.Y + `"}`))
	return base64.RawURLEncoding.EncodeToString(sum[:])
}

type clientKeyRecord struct {
	Kid       string        `json:"kid"`
	ClientID  string        `json:"client_id"`
	TenantID  string        `json:"tenant_id"`
	Alg       string        `json:"alg"`
	JWK       ecJWK         `json:"jwk"`
	Role      entities.Role `json:"role"`
	Scopes    []string      `json:"scopes"`
	Active    *bool         `json:"active"`
	CreatedAt string        `json:"created_at"`
	CreatedBy string        `json:"created_by"`
}

type clientKeyRevocation struct {
	Kid       string `json:"kid"`
	RevokedAt string `json:"revoked_at"`
	RevokedBy string `json:"revoked_by"`
}

// clientKeyState is one kid folded from clientkey:<tenant>. A revocation is
// permanent: a later registration of the same kid does not undo it.
type clientKeyState struct {
	record     *clientKeyRecord
	key        *ecdsa.PublicKey
	invalid    bool
	revoked    bool
	revocation clientKeyRevocation
}

func (s *clientKeyState) usable() bool {
	return s != nil && s.record != nil && s.key != nil && !s.invalid && !s.revoked
}

func sameClientKey(a, b *clientKeyRecord) bool {
	return a.ClientID == b.ClientID && a.TenantID == b.TenantID && a.Alg == b.Alg && a.JWK == b.JWK && a.Role == b.Role && slices.Equal(a.Scopes, b.Scopes)
}

func validClientID(id string) bool {
	if id == "" || len(id) > 200 {
		return false
	}
	for _, r := range id {
		if (r < 'a' || r > 'z') && (r < 'A' || r > 'Z') && (r < '0' || r > '9') && !strings.ContainsRune("._@:/-", r) {
			return false
		}
	}
	return true
}

func validTenantID(id string) bool {
	return id != "" && len(id) <= 128 && !strings.ContainsAny(id, "\x00/ ")
}

func (r *clientKeyRecord) verify(tenant string) (*ecdsa.PublicKey, error) {
	if r.TenantID != tenant || r.Alg != assertionAlg || r.Role != entities.RoleServiceAccount || r.Active == nil || !*r.Active || !validClientID(r.ClientID) || !validKeyScopes(r.Scopes) || r.JWK.D != "" {
		return nil, errors.New("invalid client key registration")
	}
	key, err := r.JWK.publicKey()
	if err != nil || r.JWK.thumbprint() != r.Kid {
		return nil, errors.New("invalid client key registration")
	}
	return key, nil
}

type clientKeyEvent struct {
	ID        string          `json:"id"`
	TenantID  string          `json:"tenant_id"`
	EntityID  string          `json:"entity_id"`
	EventType string          `json:"event_type"`
	Timestamp time.Time       `json:"timestamp"`
	Version   uint64          `json:"version"`
	Payload   json.RawMessage `json:"payload"`
}

var errInvalidClientKeyHistory = errors.New("invalid API key history")

// foldClientKeys applies the same retained-entity rules as managedKey: one bad
// event structure invalidates the whole history, while a registration that
// fails validation only disables its own kid.
func foldClientKeys(tenant string, events []clientKeyEvent, entityVersion uint64) (map[string]*clientKeyState, error) {
	entity := clientKeyEntityPrefix + tenant
	states := map[string]*clientKeyState{}
	seen := map[string]bool{}
	var previous uint64
	var numbered bool
	var timestamp time.Time
	for _, event := range events {
		legacy := !numbered && event.Version == 1
		if event.ID == "" || seen[event.ID] || event.TenantID != tenant || event.EntityID != entity || event.Timestamp.IsZero() || event.Timestamp.Before(timestamp) || (!legacy && event.Version != previous+1) {
			return nil, errInvalidClientKeyHistory
		}
		seen[event.ID] = true
		previous++
		numbered = numbered || event.Version > 1
		timestamp = event.Timestamp
		switch event.EventType {
		case clientKeyRegistered:
			var record clientKeyRecord
			if json.Unmarshal(event.Payload, &record) != nil || record.Kid == "" || record.TenantID != tenant {
				return nil, errInvalidClientKeyHistory
			}
			state := states[record.Kid]
			if state == nil {
				state = &clientKeyState{}
				states[record.Kid] = state
			}
			key, err := record.verify(tenant)
			switch {
			case err != nil:
				state.invalid = true
			case state.record == nil:
				state.record, state.key = &record, key
			case !sameClientKey(state.record, &record):
				state.invalid = true
			}
		case clientKeyRevoked:
			var revocation clientKeyRevocation
			if json.Unmarshal(event.Payload, &revocation) != nil || revocation.Kid == "" {
				return nil, errInvalidClientKeyHistory
			}
			state := states[revocation.Kid]
			if state == nil {
				state = &clientKeyState{}
				states[revocation.Kid] = state
			}
			if !state.revoked {
				state.revoked, state.revocation = true, revocation
			}
		default:
			return nil, errInvalidClientKeyHistory
		}
	}
	if len(events) > 0 && events[len(events)-1].Version != entityVersion {
		return nil, errInvalidClientKeyHistory
	}
	return states, nil
}

func (a *AuthClient) lifecycleBase(tenant string) string {
	if a.lifecycleCoreURL != nil {
		return a.lifecycleCoreURL(tenant)
	}
	return a.coreURL
}

// clientKeyStates reads clientkey:<tenant> under Core's strict retained-entity
// contract, the same read managedKey makes for apikey:<tenant>.
func (a *AuthClient) clientKeyStates(ctx context.Context, tenant string) (map[string]*clientKeyState, error) {
	base := a.lifecycleBase(tenant)
	if base == "" {
		return nil, errKeyAuthorityUnavailable
	}
	u, err := url.Parse(strings.TrimRight(base, "/") + queryEventsPath)
	if err != nil {
		return nil, errKeyAuthorityUnavailable
	}
	entity := clientKeyEntityPrefix + tenant
	q := u.Query()
	q.Set("tenant_id", tenant)
	q.Set("entity_id", entity)
	q.Set("limit", "1001")
	q.Set("integrity", retainedEntityIntegrity)
	u.RawQuery = q.Encode()
	ctx, cancel := context.WithTimeout(ctx, corehold.Deadline+5*time.Second)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String(), http.NoBody)
	if err != nil {
		return nil, errKeyAuthorityUnavailable
	}
	bearer, err := a.SignDelegationJWT("control-plane", tenant, entities.RoleAdmin)
	if err != nil {
		return nil, errKeyAuthorityUnavailable
	}
	req.Header.Set("Authorization", "Bearer "+bearer)
	var resp *http.Response
	corehold.Hold(ctx, func() bool {
		if resp != nil {
			closeResponseBody(resp)
			resp = nil
		}
		resp, err = a.http.Do(req) //nolint:bodyclose // closed by the next attempt or the deferred close below
		if err != nil {
			return corehold.Unreached(err)
		}
		return corehold.ResponseNotReady(resp)
	})
	if err != nil {
		return nil, errKeyAuthorityUnavailable
	}
	defer closeResponseBody(resp)
	if resp.StatusCode != http.StatusOK {
		return nil, errKeyAuthorityUnavailable
	}
	var page struct {
		Events    []clientKeyEvent `json:"events"`
		Count     *int             `json:"count"`
		Total     *int             `json:"total_count"`
		HasMore   *bool            `json:"has_more"`
		Version   *uint64          `json:"entity_version"`
		Integrity struct {
			Protocol string `json:"protocol"`
			TenantID string `json:"tenant_id"`
			EntityID string `json:"entity_id"`
		} `json:"archive_integrity"`
	}
	decoder := json.NewDecoder(io.LimitReader(resp.Body, 2*1024*1024))
	if decoder.Decode(&page) != nil || decoder.Decode(new(any)) != io.EOF || page.Count == nil || page.Total == nil || page.HasMore == nil || page.Version == nil || *page.HasMore || *page.Count != len(page.Events) || *page.Total != len(page.Events) || len(page.Events) > 1000 || page.Integrity.Protocol != retainedEntityIntegrity || page.Integrity.TenantID != tenant || page.Integrity.EntityID != entity {
		return nil, errors.New("incomplete API key authority")
	}
	return foldClientKeys(tenant, page.Events, *page.Version)
}

// assertedTenantEntry caches one folded clientkey:<tenant> read, so a single
// Core read answers every kid of that tenant.
type assertedTenantEntry struct {
	states           map[string]*clientKeyState
	err              error
	fetched          time.Time
	unavailableUntil time.Time
	good             map[string]*clientKeyState
	goodAt           time.Time
	// revoked outlives every later read: a revocation observed once is never
	// undone by an older or lagging answer.
	revoked map[string]bool
	// generation moves on every operator write; a read that started before the
	// write is answered to its callers but never cached.
	generation uint64
}

type assertedLookup struct {
	done   chan struct{}
	states map[string]*clientKeyState
	err    error
}

type assertedKeys struct {
	lookup func(context.Context, string) (map[string]*clientKeyState, error)
	now    func() time.Time

	mu       sync.Mutex
	cache    map[string]*assertedTenantEntry
	inflight map[string]*assertedLookup
	// Token bucket for cache-miss Core reads: the lookup runs before the
	// signature is verified, so an invented tenant must not cost a read each.
	tokens   float64
	refilled time.Time
}

func newAssertedKeys(lookup func(context.Context, string) (map[string]*clientKeyState, error)) *assertedKeys {
	return &assertedKeys{lookup: lookup, now: time.Now, cache: map[string]*assertedTenantEntry{}, inflight: map[string]*assertedLookup{}, tokens: assertedKeyLookupBurst}
}

func (k *assertedKeys) entry(tenant string) *assertedTenantEntry {
	entry := k.cache[tenant]
	if entry == nil {
		k.evict()
		entry = &assertedTenantEntry{revoked: map[string]bool{}}
		k.cache[tenant] = entry
	}
	return entry
}

func (k *assertedKeys) forget(tenant, kid string) {
	k.mu.Lock()
	defer k.mu.Unlock()
	entry := k.entry(tenant)
	entry.generation++
	entry.fetched, entry.unavailableUntil = time.Time{}, time.Time{}
	delete(entry.good, kid)
}

// evict makes room without discarding a key that the outage window could
// still serve, unless every entry holds one.
func (k *assertedKeys) evict() {
	if len(k.cache) < assertedKeyCacheLimit {
		return
	}
	now := k.now()
	var oldest string
	for tenant, entry := range k.cache {
		if k.inflight[tenant] != nil {
			continue
		}
		if len(entry.good) == 0 || now.Sub(entry.goodAt) >= assertedKeyStaleLimit {
			delete(k.cache, tenant)
			continue
		}
		if oldest == "" || entry.fetched.Before(k.cache[oldest].fetched) {
			oldest = tenant
		}
	}
	if len(k.cache) >= assertedKeyCacheLimit && oldest != "" {
		delete(k.cache, oldest)
	}
}

func (k *assertedKeys) takeLookup(now time.Time) bool {
	if !k.refilled.IsZero() {
		k.tokens = min(assertedKeyLookupBurst, k.tokens+now.Sub(k.refilled).Seconds()*assertedKeyLookupRate)
	}
	k.refilled = now
	if k.tokens < 1 {
		return false
	}
	k.tokens--
	return true
}

func stale(entry *assertedTenantEntry, kid string, now time.Time) (*clientKeyState, error) {
	if entry != nil && !entry.revoked[kid] && entry.good[kid] != nil && now.Sub(entry.goodAt) < assertedKeyStaleLimit {
		return entry.good[kid], nil
	}
	return nil, errKeyAuthorityUnavailable
}

func answer(entry *assertedTenantEntry, kid string, states map[string]*clientKeyState, err error, now time.Time) (*clientKeyState, error) {
	if errors.Is(err, errKeyAuthorityUnavailable) {
		return stale(entry, kid, now)
	}
	if err != nil {
		return nil, err
	}
	state := states[kid]
	if entry != nil && entry.revoked[kid] && (state == nil || !state.revoked) {
		return &clientKeyState{revoked: true}, nil
	}
	return state, nil
}

func (k *assertedKeys) store(tenant string, generation uint64, states map[string]*clientKeyState, err error, now time.Time) {
	entry := k.entry(tenant)
	if entry.generation != generation {
		return
	}
	if errors.Is(err, errKeyAuthorityUnavailable) {
		entry.unavailableUntil = now.Add(keyAuthorityUnavailableBackoff)
		return
	}
	entry.states, entry.err, entry.fetched, entry.unavailableUntil = states, err, now, time.Time{}
	entry.good, entry.goodAt = map[string]*clientKeyState{}, now
	for kid, state := range states {
		if state.revoked {
			entry.revoked[kid] = true
		}
		if err == nil && state.usable() && !entry.revoked[kid] {
			entry.good[kid] = state
		}
	}
}

// resolve answers from the per-tenant cache, joining a Core read already in
// flight for the tenant instead of starting another.
func (k *assertedKeys) resolve(ctx context.Context, tenant, kid string) (*clientKeyState, error) {
	now := k.now()
	k.mu.Lock()
	entry := k.cache[tenant]
	if entry != nil && now.Before(entry.unavailableUntil) {
		defer k.mu.Unlock()
		return stale(entry, kid, now)
	}
	if entry != nil && !entry.fetched.IsZero() && now.Sub(entry.fetched) < assertedKeyCacheTTL {
		defer k.mu.Unlock()
		return answer(entry, kid, entry.states, entry.err, now)
	}
	if call := k.inflight[tenant]; call != nil {
		k.mu.Unlock()
		select {
		case <-call.done:
		case <-ctx.Done():
			return nil, errKeyAuthorityUnavailable
		}
		k.mu.Lock()
		defer k.mu.Unlock()
		return answer(k.cache[tenant], kid, call.states, call.err, now)
	}
	if !k.takeLookup(now) {
		defer k.mu.Unlock()
		return stale(entry, kid, now)
	}
	var generation uint64
	if entry != nil {
		generation = entry.generation
	}
	call := &assertedLookup{done: make(chan struct{})}
	k.inflight[tenant] = call
	k.mu.Unlock()

	// Detached from the request: callers that join this read must not inherit
	// one client's disconnect as an outage. clientKeyStates bounds the read.
	call.states, call.err = k.lookup(context.WithoutCancel(ctx), tenant)

	k.mu.Lock()
	defer k.mu.Unlock()
	delete(k.inflight, tenant)
	k.store(tenant, generation, call.states, call.err, now)
	close(call.done)
	return answer(k.cache[tenant], kid, call.states, call.err, now)
}

// assertionClaims is the client-signed claim set. Role and scope claims are
// deliberately absent: only the registration grants them.
type assertionClaims struct {
	Issuer    string          `json:"iss"`
	Subject   string          `json:"sub"`
	Audience  json.RawMessage `json:"aud"`
	IssuedAt  *int64          `json:"iat"`
	NotAfter  *int64          `json:"exp"`
	NotBefore *int64          `json:"nbf"`
	JTI       string          `json:"jti"`
	TenantID  *string         `json:"tenant_id"`
}

// Valid is a no-op; verifyAssertionClaims applies the assertion rules.
func (assertionClaims) Valid() error { return nil }

// assertedHeader routes a bearer to the asserted path only when its unverified
// header names ES256 with a kid; every other token keeps the HS256 path.
func assertedHeader(token string) (string, bool) {
	if strings.Count(token, ".") != 2 {
		return "", false
	}
	parsed, _, err := new(jwt.Parser).ParseUnverified(token, jwt.MapClaims{})
	if err != nil {
		return "", false
	}
	alg, algOK := parsed.Header["alg"].(string)
	kid, kidOK := parsed.Header["kid"].(string)
	typ, typSet := parsed.Header["typ"]
	typName, typOK := typ.(string)
	if !algOK || alg != assertionAlg || !kidOK || kid == "" || (typSet && (!typOK || typName != "JWT")) {
		return "", false
	}
	return kid, true
}

func audienceMatches(raw json.RawMessage, want string) bool {
	trimmed := bytes.TrimSpace(raw)
	if len(trimmed) == 0 || trimmed[0] != '"' {
		return false
	}
	var audience string
	return json.Unmarshal(trimmed, &audience) == nil && audience == want
}

type assertionDenialError struct {
	status  int
	reason  string
	message string
}

func (d *assertionDenialError) Error() string { return d.reason + ": " + d.message }

func deny(status int, reason, message string) *assertionDenialError {
	return &assertionDenialError{status: status, reason: reason, message: message}
}

func verifyAssertionClaims(claims *assertionClaims, record *clientKeyRecord, audience string, now time.Time) *assertionDenialError {
	if claims.Issuer != record.ClientID || record.TenantID != claims.Subject || (claims.TenantID != nil && *claims.TenantID != claims.Subject) {
		return deny(http.StatusUnauthorized, denialAssertedUnregistered, "assertion issuer or tenant does not match the registered client key")
	}
	if !audienceMatches(claims.Audience, audience) {
		return deny(http.StatusUnauthorized, denialAssertedAudience, "assertion audience must be the single string "+audience)
	}
	if claims.NotAfter == nil {
		return deny(http.StatusUnauthorized, denialAssertedExpired, "assertion has no exp")
	}
	if claims.IssuedAt == nil {
		return deny(http.StatusUnauthorized, denialAssertedSkew, "assertion has no iat")
	}
	iat, exp := time.Unix(*claims.IssuedAt, 0), time.Unix(*claims.NotAfter, 0)
	if !now.Before(exp.Add(assertionClockSkew)) {
		return deny(http.StatusUnauthorized, denialAssertedExpired, "assertion has expired")
	}
	if !exp.After(iat) || exp.Sub(iat) > assertionMaxLifetime || iat.After(now.Add(assertionClockSkew)) {
		return deny(http.StatusUnauthorized, denialAssertedSkew, "assertion iat and exp must be at most 60s apart and within 30s of the gateway clock")
	}
	if claims.NotBefore != nil && time.Unix(*claims.NotBefore, 0).After(now.Add(assertionClockSkew)) {
		return deny(http.StatusUnauthorized, denialAssertedSkew, "assertion is not yet valid")
	}
	if claims.JTI == "" {
		return deny(http.StatusUnauthorized, denialAssertedSignature, "assertion has no jti")
	}
	return nil
}

var assertionParser = &jwt.Parser{ValidMethods: []string{assertionAlg}, SkipClaimsValidation: true}

// verifyAssertion returns the verified registration and claims, or a denial and
// the tenant label it is counted under ("" until the signature is verified).
func (a *AuthClient) verifyAssertion(ctx context.Context, token, kid string) (*clientKeyRecord, *assertionClaims, string, *assertionDenialError) {
	var unverified assertionClaims
	if _, _, err := assertionParser.ParseUnverified(token, &unverified); err != nil || !validKid(kid) || unverified.Subject == "" || unverified.Issuer == "" || !validTenantID(unverified.Subject) {
		return nil, nil, "", deny(http.StatusUnauthorized, denialAssertedSignature, "malformed client assertion")
	}
	state, err := a.asserted.resolve(ctx, unverified.Subject, kid)
	switch {
	case errors.Is(err, errKeyAuthorityUnavailable):
		return nil, nil, "", deny(http.StatusServiceUnavailable, denialAssertedUnavailable, "client key authority is unavailable; retry shortly")
	case err != nil:
		return nil, nil, "", deny(http.StatusUnauthorized, denialReason(err), "client key history is invalid")
	case state == nil:
		return nil, nil, "", deny(http.StatusUnauthorized, denialAssertedUnregistered, "client key is not registered for this tenant")
	case state.invalid:
		return nil, nil, "", deny(http.StatusUnauthorized, denialHistory, "client key registration is invalid")
	case state.key == nil:
		return nil, nil, "", deny(http.StatusUnauthorized, denialAssertedRevoked, "client key has been revoked")
	}
	var claims assertionClaims
	parsed, err := assertionParser.ParseWithClaims(token, &claims, func(t *jwt.Token) (any, error) {
		method, ok := t.Method.(*jwt.SigningMethodECDSA)
		if !ok || method.Alg() != assertionAlg {
			return nil, fmt.Errorf("unexpected signing method %v", t.Header["alg"])
		}
		return state.key, nil
	})
	if err != nil || !parsed.Valid {
		return nil, nil, "", deny(http.StatusUnauthorized, denialAssertedSignature, "client assertion signature is invalid")
	}
	if state.revoked {
		return nil, nil, claims.Subject, deny(http.StatusUnauthorized, denialAssertedRevoked, "client key has been revoked")
	}
	if denial := verifyAssertionClaims(&claims, state.record, a.assertionAudience, a.asserted.now()); denial != nil {
		return nil, nil, claims.Subject, denial
	}
	return state.record, &claims, claims.Subject, nil
}

func assertedKeyAllows(c *gin.Context, record *clientKeyRecord) bool {
	return managedKeyAllows(c, &managedKeyRecord{Scopes: record.Scopes}, record.Role)
}

// authenticateAssertion always enforces, whatever ALLSOURCE_KEY_AUTHORITY_MODE
// says, and never falls back to the HS256 path. It writes the refusal itself.
func (a *AuthClient) authenticateAssertion(c *gin.Context, token, kid string) (*Claims, bool) {
	record, claims, tenant, denial := a.verifyAssertion(c.Request.Context(), token, kid)
	if denial == nil && !assertedKeyAllows(c, record) {
		denial = deny(http.StatusForbidden, denialScope, "insufficient API key scope")
	}
	if denial != nil {
		label := tenant
		if label == "" {
			label = "unverified"
		}
		a.keys.recordAs(keyAuthorityEnforce, label, denial.reason, denial)
		if denial.status == http.StatusServiceUnavailable {
			c.Header("Retry-After", "5")
		}
		errorCode := "unauthorized"
		switch denial.status {
		case http.StatusForbidden:
			errorCode = "insufficient API key scope"
		case http.StatusServiceUnavailable:
			errorCode = "authority_unavailable"
		}
		c.AbortWithStatusJSON(denial.status, gin.H{"error": errorCode, "reason": denial.reason, "message": denial.message})
		return nil, false
	}
	c.Set("auth_assertion_jti", claims.JTI)
	return &Claims{UserID: record.ClientID + ":" + kid, TenantID: claims.Subject, Role: record.Role, IsAPIKey: true, CredentialKind: clientAssertionKind}, true
}

func logClientKeyWrite(action, tenant, kid, actor string) {
	log.Printf("client key %s: tenant=%s kid=%s by=%s", action, tenant, kid, actor) //nolint:gosec // G706: tenant and kid are validated, actor comes from a verified admin token.
}
