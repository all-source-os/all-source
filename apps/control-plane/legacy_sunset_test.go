package main

import (
	"encoding/json"
	"net/http"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	dto "github.com/prometheus/client_model/go"
)

func legacyServed(tenant string) float64 {
	var metric dto.Metric
	if err := legacyKeyRequests.WithLabelValues(tenant).Write(&metric); err != nil {
		return -1
	}
	return metric.GetCounter().GetValue()
}

func TestLegacySunsetServesUnregisteredKeysUntilTheInstant(t *testing.T) {
	gin.SetMode(gin.TestMode)
	for _, mode := range []keyAuthorityMode{keyAuthorityEnforce, keyAuthorityShadow} {
		t.Run(string(mode), func(t *testing.T) {
			tenant := "sunset-tenant-" + string(mode)
			core, _ := keyAuthorityCore(t, http.StatusOK, emptyKeyPage(tenant))
			client := NewAuthClient("synthetic-secret", core.URL)
			client.SetKeyAuthorityMode(mode)
			sunset := time.Date(2026, 12, 7, 0, 0, 0, 0, time.UTC)
			client.SetLegacyKeySunset(sunset)
			clock := sunset.Add(-time.Hour)
			client.legacy.now = func() time.Time { return clock }

			before := legacyServed(tenant)
			for range 2 {
				if got := lifecycleRequest(t, client, legacyOnboardingClaims(tenant), http.MethodGet, queryEventsPath, "").Code; got != http.StatusOK {
					t.Fatalf("legacy key refused before the sunset: %d", got)
				}
			}
			if got := legacyServed(tenant) - before; got != 2 {
				t.Fatalf("legacy requests counted %v, want 2", got)
			}

			clock = sunset
			recorder := lifecycleRequest(t, client, legacyOnboardingClaims(tenant), http.MethodGet, queryEventsPath, "")
			var body map[string]string
			if err := json.Unmarshal(recorder.Body.Bytes(), &body); err != nil {
				t.Fatal(err)
			}
			if recorder.Code != http.StatusUnauthorized || body["error"] != "unauthorized" || body["reason"] != "legacy_key_sunset" ||
				body["message"] != "This API key predates registered keys and stopped working on 2026-12-07. Register a client key or create a dashboard key." {
				t.Fatalf("after the sunset: %d %v", recorder.Code, body)
			}
		})
	}
}

func TestLegacySunsetNeverServesARevokedKey(t *testing.T) {
	gin.SetMode(gin.TestMode)
	revoked := map[string]any{"id": "revoked", "tenant_id": "proof-tenant", "entity_id": "apikey:proof-tenant", "event_type": "apikey.revoked", "version": 2,
		"timestamp": "2026-01-01T00:00:01Z", "payload": map[string]any{"key_id": "proof-key"}}
	core, _ := keyAuthorityCore(t, http.StatusOK, lifecyclePage(lifecycleCreated(), revoked))
	client := NewAuthClient("synthetic-secret", core.URL)
	client.SetLegacyKeySunset(time.Now().Add(30 * 24 * time.Hour))
	if got := lifecycleRequest(t, client, lifecycleClaims(), http.MethodGet, queryEventsPath, "").Code; got != http.StatusUnauthorized {
		t.Fatalf("revoked key served before the sunset: %d", got)
	}
}

func TestLegacySunsetNeverServesALegacyKeyATenantRevoked(t *testing.T) {
	gin.SetMode(gin.TestMode)
	stray := map[string]any{"id": "stray", "tenant_id": "proof-tenant", "entity_id": "apikey:proof-tenant", "event_type": "apikey.revoked", "version": 1,
		"timestamp": "2026-01-01T00:00:00Z", "payload": map[string]any{"key_id": "proof-tenant"}}
	for mode, want := range map[keyAuthorityMode]int{keyAuthorityEnforce: http.StatusUnauthorized, keyAuthorityShadow: http.StatusOK} {
		core, _ := keyAuthorityCore(t, http.StatusOK, lifecyclePage(stray))
		client := NewAuthClient("synthetic-secret", core.URL)
		client.SetKeyAuthorityMode(mode)
		client.SetLegacyKeySunset(time.Now().Add(30 * 24 * time.Hour))
		before := legacyServed("proof-tenant")
		if got := lifecycleRequest(t, client, legacyOnboardingClaims("proof-tenant"), http.MethodGet, queryEventsPath, "").Code; got != want {
			t.Fatalf("%s: legacy key named by a revocation got %d, want %d", mode, got, want)
		}
		if served := legacyServed("proof-tenant") - before; served != 0 {
			t.Fatalf("%s: a revoked legacy key was counted as served by the sunset", mode)
		}
	}
}

func TestParseLegacyKeySunset(t *testing.T) {
	if at, err := parseLegacyKeySunset(""); err != nil || !at.IsZero() {
		t.Fatal("unset sunset must be the zero time")
	}
	if at, err := parseLegacyKeySunset("2026-12-07T00:00:00+02:00"); err != nil || !at.Equal(time.Date(2026, 12, 6, 22, 0, 0, 0, time.UTC)) {
		t.Fatalf("got %v, %v", at, err)
	}
	if _, err := parseLegacyKeySunset("2026-12-07"); err == nil {
		t.Fatal("a date without a time accepted")
	}
}
