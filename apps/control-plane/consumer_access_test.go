package main

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"
)

func consumerAccessServer(t *testing.T, accepted map[string]bool, calls *atomic.Int32) *httptest.Server {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		calls.Add(1)
		if r.URL.Path != "/api/v1/events/query" {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		if accepted[r.Header.Get("Authorization")] {
			w.WriteHeader(http.StatusOK)
			return
		}
		w.WriteHeader(http.StatusUnauthorized)
	}))
	t.Cleanup(server.Close)
	return server
}

func probeAgainst(server *httptest.Server, canaries []consumerCanary) *consumerAccessProbe {
	return &consumerAccessProbe{canaries: canaries, base: server.URL, client: server.Client()}
}

func serveConsumerAccess(t *testing.T, probe *consumerAccessProbe) *httptest.ResponseRecorder {
	t.Helper()
	gin.SetMode(gin.TestMode)
	engine := gin.New()
	engine.GET(pathConsumerAccess, probe.handle)
	recorder := httptest.NewRecorder()
	request := httptest.NewRequestWithContext(context.Background(), http.MethodGet, pathConsumerAccess, http.NoBody)
	engine.ServeHTTP(recorder, request)
	return recorder
}

func TestConsumerAccessFailsClosedWithoutCanaries(t *testing.T) {
	var calls atomic.Int32
	probe := probeAgainst(consumerAccessServer(t, nil, &calls), nil)

	recorder := serveConsumerAccess(t, probe)

	if recorder.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want 503 when no canary is configured", recorder.Code)
	}
	if calls.Load() != 0 {
		t.Fatalf("probe made %d requests with no canaries", calls.Load())
	}
}

func TestConsumerAccessPassesWhenEveryKindReads(t *testing.T) {
	var calls atomic.Int32
	server := consumerAccessServer(t, map[string]bool{"Bearer old": true, "Bearer managed": true}, &calls)
	probe := probeAgainst(server, []consumerCanary{{Kind: "onboarding_api_key", Token: "old"}, {Kind: "managed_api_key", Token: "managed"}})

	recorder := serveConsumerAccess(t, probe)

	if recorder.Code != http.StatusOK {
		t.Fatalf("status = %d, want 200 when every kind reads; body %s", recorder.Code, recorder.Body.String())
	}
}

func TestConsumerAccessFailsWhenOneKindIsRejected(t *testing.T) {
	var calls atomic.Int32
	server := consumerAccessServer(t, map[string]bool{"Bearer managed": true}, &calls)
	probe := probeAgainst(server, []consumerCanary{{Kind: "onboarding_api_key", Token: "old"}, {Kind: "managed_api_key", Token: "managed"}})

	result := probe.result(context.Background())

	if result.OK {
		t.Fatal("result OK while the onboarding key kind was rejected")
	}
	if result.Kinds[0].Status != http.StatusUnauthorized || result.Kinds[1].Status != http.StatusOK {
		t.Fatalf("kinds = %+v, want onboarding 401 and managed 200", result.Kinds)
	}
	if recorder := serveConsumerAccess(t, probe); recorder.Code != http.StatusServiceUnavailable {
		t.Fatalf("status = %d, want 503", recorder.Code)
	}
}

func TestConsumerAccessReusesResultWithinWindow(t *testing.T) {
	var calls atomic.Int32
	server := consumerAccessServer(t, map[string]bool{"Bearer managed": true}, &calls)
	probe := probeAgainst(server, []consumerCanary{{Kind: "managed_api_key", Token: "managed"}})

	probe.result(context.Background())
	probe.result(context.Background())
	if calls.Load() != 1 {
		t.Fatalf("made %d probe requests inside one cache window, want 1", calls.Load())
	}

	probe.cachedAt = time.Now().Add(-consumerAccessCacheTTL)
	probe.result(context.Background())
	if calls.Load() != 2 {
		t.Fatalf("made %d probe requests after the window expired, want 2", calls.Load())
	}
}

func TestParseConsumerCanariesDropsIncompleteEntries(t *testing.T) {
	canaries := parseConsumerCanaries(`[{"kind":"managed_api_key","token":"t"},{"kind":"","token":"x"},{"kind":"k"}]`)
	if len(canaries) != 1 || canaries[0].Kind != "managed_api_key" {
		t.Fatalf("canaries = %+v, want only the complete entry", canaries)
	}
	if parseConsumerCanaries("not json") != nil {
		t.Fatal("malformed JSON produced canaries")
	}
}

func TestConsumerAccessSignsAnAssertedCanaryEveryRound(t *testing.T) {
	f := newClientKeyFixture(t)
	private, err := json.Marshal(f.privateJWK())
	if err != nil {
		t.Fatal(err)
	}
	raw := `[{"kind":"managed_api_key","token":"managed"},{"kind":"asserted_client_key","client_id":"` + assertedClient + `","tenant":"` + assertedTenant + `","kid":"` + f.kid + `","private_jwk":` + string(private) + `}]`
	canaries := parseConsumerCanaries(raw)
	if len(canaries) != 2 || !canaries[1].asserted() {
		t.Fatalf("canaries = %+v, want the token and the asserted entry", canaries)
	}
	var seen []string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		bearer := strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")
		if bearer == "managed" {
			w.WriteHeader(http.StatusOK)
			return
		}
		var claims assertionClaims
		parsed, err := assertionParser.ParseWithClaims(bearer, &claims, func(*jwt.Token) (any, error) { return f.jwk.publicKey() })
		if err != nil || !parsed.Valid || parsed.Header["kid"] != f.kid || claims.Subject != assertedTenant || claims.Issuer != assertedClient ||
			!audienceMatches(claims.Audience, defaultAssertionAudience) || *claims.NotAfter-*claims.IssuedAt != 60 || claims.JTI == "" {
			w.WriteHeader(http.StatusUnauthorized)
			return
		}
		seen = append(seen, claims.JTI)
		w.WriteHeader(http.StatusOK)
	}))
	t.Cleanup(server.Close)
	probe := &consumerAccessProbe{canaries: canaries, base: server.URL, audience: defaultAssertionAudience, client: server.Client()}

	if result := probe.run(context.Background()); !result.OK {
		t.Fatalf("asserted canary rejected: %+v", result)
	}
	probe.run(context.Background())
	if len(seen) != 2 || seen[0] == seen[1] {
		t.Fatalf("assertions were not fresh per round: %v", seen)
	}
}

func TestParseConsumerCanariesDropsAnAssertedEntryWithAWrongKid(t *testing.T) {
	f := newClientKeyFixture(t)
	private, err := json.Marshal(f.privateJWK())
	if err != nil {
		t.Fatal(err)
	}
	raw := `[{"kind":"asserted_client_key","client_id":"c","tenant":"t","kid":"not-the-thumbprint","private_jwk":` + string(private) + `}]`
	if canaries := parseConsumerCanaries(raw); len(canaries) != 0 {
		t.Fatalf("an asserted canary with a wrong kid was kept: %+v", canaries)
	}
}
