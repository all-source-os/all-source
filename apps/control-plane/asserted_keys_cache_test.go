package main

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/dgrijalva/jwt-go"
)

func fakeKid(i int) string {
	sum := sha256.Sum256(fmt.Appendf(nil, "kid-%d", i))
	return base64.RawURLEncoding.EncodeToString(sum[:])
}

func TestAssertedKidMustBeAThumbprintBeforeCoreIsRead(t *testing.T) {
	f := newClientKeyFixture(t)
	core := newFakeClientKeyCore(t, clientKeyRegisteredEvent(f, 1))
	client := NewAuthClient("synthetic-secret", core.server.URL)
	for _, kid := range []string{"short", strings.Repeat("A", 44), strings.Repeat("A", 42) + "=", strings.Repeat("A", 4096)} {
		token := signAssertion(t, f, func(_ jwt.MapClaims, h map[string]any) { h["kid"] = kid })
		recorder := assertedRequest(t, client, token, http.MethodGet, queryEventsPath, "")
		if recorder.Code != http.StatusUnauthorized || assertedReason(t, recorder) != denialAssertedSignature {
			t.Fatalf("kid of %d chars: %d %s", len(kid), recorder.Code, recorder.Body)
		}
	}
	if reads := core.reads.Load(); reads != 0 {
		t.Fatalf("a malformed kid cost %d Core reads", reads)
	}
}

func TestAssertedKidsOfOneTenantShareOneCoreRead(t *testing.T) {
	f := newClientKeyFixture(t)
	core := newFakeClientKeyCore(t, clientKeyRegisteredEvent(f, 1))
	client := NewAuthClient("synthetic-secret", core.server.URL)
	for i := range 50 {
		token := signAssertion(t, f, func(_ jwt.MapClaims, h map[string]any) { h["kid"] = fakeKid(i) })
		if recorder := assertedRequest(t, client, token, http.MethodGet, queryEventsPath, ""); recorder.Code != http.StatusUnauthorized {
			t.Fatalf("unregistered kid: %d", recorder.Code)
		}
	}
	if got := assertedRequest(t, client, signAssertion(t, f, nil), http.MethodGet, queryEventsPath, "").Code; got != http.StatusOK {
		t.Fatalf("registered kid after the flood: %d", got)
	}
	if reads := core.reads.Load(); reads != 1 {
		t.Fatalf("51 kids of one tenant cost %d Core reads, want 1", reads)
	}
}

func usableState(t *testing.T) map[string]*clientKeyState {
	t.Helper()
	f := newClientKeyFixture(t)
	key, err := f.jwk.publicKey()
	if err != nil {
		t.Fatal(err)
	}
	return map[string]*clientKeyState{fakeKid(0): {record: &clientKeyRecord{Kid: fakeKid(0)}, key: key}}
}

func TestAssertedConcurrentMissesJoinOneLookup(t *testing.T) {
	release := make(chan struct{})
	var calls atomic.Int64
	states := usableState(t)
	keys := newAssertedKeys(func(context.Context, string) (map[string]*clientKeyState, error) {
		calls.Add(1)
		<-release
		return states, nil
	})
	var wg sync.WaitGroup
	results := make(chan *clientKeyState, 20)
	for range 20 {
		wg.Go(func() {
			state, err := keys.resolve(t.Context(), assertedTenant, fakeKid(0))
			if err != nil {
				t.Error(err)
			}
			results <- state
		})
	}
	for {
		keys.mu.Lock()
		started := keys.inflight[assertedTenant] != nil
		keys.mu.Unlock()
		if started {
			break
		}
		time.Sleep(time.Millisecond)
	}
	time.Sleep(10 * time.Millisecond)
	close(release)
	wg.Wait()
	close(results)
	for state := range results {
		if !state.usable() {
			t.Fatal("a joined caller did not get the lookup's answer")
		}
	}
	if got := calls.Load(); got != 1 {
		t.Fatalf("20 concurrent misses ran %d lookups, want 1", got)
	}
}

func TestAssertedRevocationIsNeverUndoneByALaterAnswer(t *testing.T) {
	clock := time.Now()
	revoked := map[string]*clientKeyState{fakeKid(0): {record: &clientKeyRecord{Kid: fakeKid(0)}, revoked: true}}
	answers := []map[string]*clientKeyState{revoked, usableState(t)}
	keys := newAssertedKeys(func(context.Context, string) (map[string]*clientKeyState, error) {
		next := answers[0]
		answers = answers[1:]
		return next, nil
	})
	keys.now = func() time.Time { return clock }
	state, err := keys.resolve(t.Context(), assertedTenant, fakeKid(0))
	if err != nil {
		t.Fatal(err)
	}
	if state.usable() {
		t.Fatal("revoked key served")
	}
	clock = clock.Add(assertedKeyCacheTTL + time.Second)
	if state, err := keys.resolve(t.Context(), assertedTenant, fakeKid(0)); err != nil || state.usable() || !state.revoked {
		t.Fatalf("a lagging read brought a revoked key back: %+v %v", state, err)
	}
}

func TestAssertedReadStartedBeforeAWriteIsNotCached(t *testing.T) {
	release := make(chan struct{})
	var calls atomic.Int64
	states := usableState(t)
	keys := newAssertedKeys(func(context.Context, string) (map[string]*clientKeyState, error) {
		if calls.Add(1) == 1 {
			<-release
			return states, nil
		}
		return map[string]*clientKeyState{fakeKid(0): {record: &clientKeyRecord{Kid: fakeKid(0)}, revoked: true}}, nil
	})
	done := make(chan struct{})
	go func() {
		defer close(done)
		if _, err := keys.resolve(context.Background(), assertedTenant, fakeKid(0)); err != nil {
			t.Error(err)
		}
	}()
	for calls.Load() == 0 {
		time.Sleep(time.Millisecond)
	}
	keys.forget(assertedTenant, fakeKid(0))
	close(release)
	<-done
	state, err := keys.resolve(t.Context(), assertedTenant, fakeKid(0))
	if err != nil {
		t.Fatal(err)
	}
	if state.usable() {
		t.Fatal("a read that started before the revocation was cached and served")
	}
	if got := calls.Load(); got != 2 {
		t.Fatalf("lookups = %d, want 2", got)
	}
}

func TestAssertedLookupsForNewTenantsAreRateLimited(t *testing.T) {
	var calls atomic.Int64
	keys := newAssertedKeys(func(context.Context, string) (map[string]*clientKeyState, error) {
		calls.Add(1)
		return map[string]*clientKeyState{}, nil
	})
	clock := time.Now()
	keys.now = func() time.Time { return clock }
	unavailable := 0
	for i := range int(assertedKeyLookupBurst) + 25 {
		if _, err := keys.resolve(t.Context(), fmt.Sprintf("tenant-%d", i), fakeKid(0)); err != nil {
			unavailable++
		}
	}
	if calls.Load() != int64(assertedKeyLookupBurst) || unavailable != 25 {
		t.Fatalf("lookups = %d, unavailable = %d", calls.Load(), unavailable)
	}
	clock = clock.Add(time.Second)
	if _, err := keys.resolve(t.Context(), "tenant-late", fakeKid(0)); err != nil {
		t.Fatalf("the bucket did not refill: %v", err)
	}
}

func TestAssertedCacheEvictionKeepsServableKeys(t *testing.T) {
	keys := newAssertedKeys(nil)
	now := time.Now()
	keys.now = func() time.Time { return now }
	good := usableState(t)
	keys.cache["keeper"] = &assertedTenantEntry{good: good, goodAt: now, fetched: now, revoked: map[string]bool{}}
	for i := range assertedKeyCacheLimit - 1 {
		keys.cache[fmt.Sprintf("empty-%d", i)] = &assertedTenantEntry{fetched: now, revoked: map[string]bool{}}
	}
	keys.mu.Lock()
	keys.entry("newcomer")
	keys.mu.Unlock()
	if keys.cache["keeper"] == nil {
		t.Fatal("eviction dropped a key the outage window could still serve")
	}
	if len(keys.cache) > assertedKeyCacheLimit {
		t.Fatalf("cache grew to %d", len(keys.cache))
	}
}
