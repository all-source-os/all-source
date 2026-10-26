package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"

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
