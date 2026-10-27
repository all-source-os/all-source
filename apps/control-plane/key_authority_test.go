package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"
	dto "github.com/prometheus/client_model/go"

	"github.com/allsource/control-plane/internal/domain/entities"
)

// legacyOnboardingClaims has the shape SignAPIKey minted for onboarded tenants
// before managed keys existed: subject is the tenant id and nothing registers it.
func legacyOnboardingClaims(tenant string) *Claims {
	now := time.Now()
	return &Claims{UserID: tenant, Username: "Longhand", Name: "Longhand", TenantID: tenant, Role: entities.RoleDeveloper, IsAPIKey: true,
		StandardClaims: jwt.StandardClaims{Issuer: "allsource", Subject: tenant, IssuedAt: now.Unix(), ExpiresAt: now.Add(365 * 24 * time.Hour).Unix()}}
}

func keyAuthorityCore(t *testing.T, status int, page map[string]any) (*httptest.Server, *atomic.Int64) {
	t.Helper()
	var hits atomic.Int64
	core := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		hits.Add(1)
		if status != http.StatusOK {
			w.WriteHeader(status)
			return
		}
		if err := json.NewEncoder(w).Encode(page); err != nil {
			t.Error(err)
		}
	}))
	t.Cleanup(core.Close)
	return core, &hits
}

func emptyKeyPage(tenant string) map[string]any {
	return map[string]any{"events": []any{}, "count": 0, "total_count": 0, "has_more": false, "entity_version": 0,
		"archive_integrity": map[string]any{"protocol": "retained-entity-v1", "tenant_id": tenant, "entity_id": "apikey:" + tenant}}
}

func denials(mode keyAuthorityMode, reason, tenant string) float64 {
	var metric dto.Metric
	if err := keyAuthorityDenials.WithLabelValues(string(mode), reason, tenant).Write(&metric); err != nil {
		return -1
	}
	return metric.GetCounter().GetValue()
}

func TestKeyAuthorityModes(t *testing.T) {
	gin.SetMode(gin.TestMode)
	for _, tc := range []struct {
		mode       keyAuthorityMode
		wantStatus int
		wantHits   int64
		wantCount  float64
	}{
		{keyAuthorityShadow, http.StatusOK, 1, 2},
		{keyAuthorityEnforce, http.StatusUnauthorized, 2, 2},
		{keyAuthorityOff, http.StatusOK, 0, 0},
	} {
		t.Run(string(tc.mode), func(t *testing.T) {
			tenant := "legacy-tenant-" + string(tc.mode)
			core, hits := keyAuthorityCore(t, http.StatusOK, emptyKeyPage(tenant))
			client := NewAuthClient("synthetic-secret", core.URL)
			client.SetKeyAuthorityMode(tc.mode)
			before := denials(tc.mode, denialUnregistered, tenant)
			for range 2 {
				if got := lifecycleRequest(t, client, legacyOnboardingClaims(tenant), "GET", queryEventsPath, "").Code; got != tc.wantStatus {
					t.Fatalf("status %d, want %d", got, tc.wantStatus)
				}
			}
			if got := hits.Load(); got != tc.wantHits {
				t.Fatalf("authority lookups %d, want %d", got, tc.wantHits)
			}
			if got := denials(tc.mode, denialUnregistered, tenant) - before; got != tc.wantCount {
				t.Fatalf("denials counted %v, want %v", got, tc.wantCount)
			}
		})
	}
}

func TestKeyAuthorityShadowBacksOffDuringAnOutage(t *testing.T) {
	gin.SetMode(gin.TestMode)
	tenant := "shadow-outage-tenant"
	core, hits := keyAuthorityCore(t, http.StatusServiceUnavailable, nil)
	client := NewAuthClient("synthetic-secret", core.URL)
	client.SetKeyAuthorityMode(keyAuthorityShadow)
	now := time.Now()
	client.keys.now = func() time.Time { return now }
	claims := legacyOnboardingClaims(tenant)
	for range 3 {
		if got := lifecycleRequest(t, client, claims, "GET", queryEventsPath, "").Code; got != http.StatusOK {
			t.Fatalf("shadow refused a client while the authority was down: %d", got)
		}
	}
	if hits.Load() != 1 {
		t.Fatalf("Core read %d times during one back-off window, want 1", hits.Load())
	}
	if denials(keyAuthorityShadow, denialUnavailable, tenant) != 3 {
		t.Fatal("unavailable authority not counted per request")
	}
	now = now.Add(keyAuthorityUnavailableBackoff + time.Second)
	if got := lifecycleRequest(t, client, claims, "GET", queryEventsPath, "").Code; got != http.StatusOK {
		t.Fatalf("shadow refused after the back-off: %d", got)
	}
	if hits.Load() != 2 {
		t.Fatalf("outage held past the back-off: %d lookups", hits.Load())
	}
}

func TestKeyAuthorityShadowRefusesARevokedKey(t *testing.T) {
	gin.SetMode(gin.TestMode)
	revoked := map[string]any{"id": "revoked", "tenant_id": "proof-tenant", "entity_id": "apikey:proof-tenant", "event_type": "apikey.revoked", "version": 2,
		"timestamp": "2026-01-01T00:00:01Z", "payload": map[string]any{"key_id": "proof-key"}}
	core, _ := keyAuthorityCore(t, http.StatusOK, lifecyclePage(lifecycleCreated(), revoked))
	client := NewAuthClient("synthetic-secret", core.URL)
	client.SetKeyAuthorityMode(keyAuthorityShadow)
	before := denials(keyAuthorityShadow, denialRevoked, "proof-tenant")
	if got := lifecycleRequest(t, client, lifecycleClaims(), "GET", queryEventsPath, "").Code; got != http.StatusUnauthorized {
		t.Fatalf("shadow served a revoked key: %d", got)
	}
	if got := denials(keyAuthorityShadow, denialRevoked, "proof-tenant") - before; got != 1 {
		t.Fatalf("revocation counted %v times under %q, want 1", got, denialRevoked)
	}
	if got := denials(keyAuthorityShadow, denialUnregistered, "proof-tenant"); got != 0 {
		t.Fatalf("revocation counted as unregistered: %v", got)
	}
}

func TestKeyAuthorityShadowServesALegacyKeyNamedByARevocation(t *testing.T) {
	gin.SetMode(gin.TestMode)
	stray := map[string]any{"id": "stray", "tenant_id": "proof-tenant", "entity_id": "apikey:proof-tenant", "event_type": "apikey.revoked", "version": 1,
		"timestamp": "2026-01-01T00:00:00Z", "payload": map[string]any{"key_id": "proof-tenant"}}
	core, _ := keyAuthorityCore(t, http.StatusOK, lifecyclePage(stray))
	client := NewAuthClient("synthetic-secret", core.URL)
	client.SetKeyAuthorityMode(keyAuthorityShadow)
	before := denials(keyAuthorityShadow, denialUnregistered, "proof-tenant")
	if got := lifecycleRequest(t, client, legacyOnboardingClaims("proof-tenant"), "GET", queryEventsPath, "").Code; got != http.StatusOK {
		t.Fatalf("shadow refused a legacy key because a revocation named its id: %d", got)
	}
	if got := denials(keyAuthorityShadow, denialUnregistered, "proof-tenant") - before; got != 1 {
		t.Fatalf("legacy key counted %v times as unregistered, want 1", got)
	}
}

func TestKeyAuthorityShadowCountsCredentialKind(t *testing.T) {
	gin.SetMode(gin.TestMode)
	client := NewAuthClient("synthetic-secret", "")
	client.SetKeyAuthorityMode(keyAuthorityShadow)
	markerless := lifecycleClaims()
	markerless.IsAPIKey = false
	markerless.TenantID = "markerless-tenant"
	if got := lifecycleRequest(t, client, markerless, "GET", queryEventsPath, "").Code; got != http.StatusOK {
		t.Fatalf("shadow refused a credential the previous release accepted: %d", got)
	}
	if denials(keyAuthorityShadow, denialKind, "markerless-tenant") != 1 {
		t.Fatal("credential kind refusal not counted")
	}
}

func TestParseKeyAuthorityMode(t *testing.T) {
	for _, tc := range []struct {
		raw  string
		want keyAuthorityMode
	}{{"", keyAuthorityShadow}, {" Shadow ", keyAuthorityShadow}, {"off", keyAuthorityOff}, {"ENFORCE", keyAuthorityEnforce}} {
		raw, want := tc.raw, tc.want
		if got, err := parseKeyAuthorityMode(raw); err != nil || got != want {
			t.Fatalf("%q: got %q, %v", raw, got, err)
		}
	}
	if _, err := parseKeyAuthorityMode("on"); err == nil {
		t.Fatal("unknown mode accepted")
	}
}
