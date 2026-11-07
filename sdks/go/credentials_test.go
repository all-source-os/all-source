package allsource

import (
	"context"
	"crypto/elliptic"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
)

func TestProviderBearerIsAskedOnEveryAttemptAndXAPIKeyIsNotSent(t *testing.T) {
	var asked, served atomic.Int32
	var mu sync.Mutex
	var seen []string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		seen = append(seen, r.Header.Get("Authorization"))
		mu.Unlock()
		if v := r.Header.Get("X-API-Key"); v != "" {
			t.Errorf("X-API-Key sent on the provider path: %q", v)
		}
		if served.Add(1) == 1 {
			w.WriteHeader(http.StatusServiceUnavailable)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"status":"ok"}`))
	}))
	defer srv.Close()

	provider := CredentialProviderFunc(func(context.Context) (string, error) {
		return fmt.Sprintf("tok-%d", asked.Add(1)), nil
	})
	client := New("", srv.URL, WithCredentialProvider(provider), fastRetry())

	if _, err := client.Health(t.Context()); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if got := asked.Load(); got != 2 {
		t.Fatalf("provider asked %d times, want 2 (once per attempt)", got)
	}
	want := []string{"Bearer tok-1", "Bearer tok-2"}
	mu.Lock()
	defer mu.Unlock()
	if strings.Join(seen, ",") != strings.Join(want, ",") {
		t.Fatalf("Authorization headers %v, want %v", seen, want)
	}
}

func TestProviderErrorFailsTheRequestWithoutRetryOrSend(t *testing.T) {
	var hits atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits.Add(1)
	}))
	defer srv.Close()

	cause := errors.New("signing key unavailable")
	var asked atomic.Int32
	provider := CredentialProviderFunc(func(context.Context) (string, error) {
		asked.Add(1)
		return "", cause
	})
	client := New("", srv.URL, WithCredentialProvider(provider), fastRetry())

	_, err := client.Health(t.Context())
	var credErr *CredentialError
	if !errors.As(err, &credErr) {
		t.Fatalf("error %v, want *CredentialError", err)
	}
	if !errors.Is(err, cause) {
		t.Fatalf("error %v does not wrap the provider's error", err)
	}
	if asked.Load() != 1 {
		t.Fatalf("provider asked %d times, want 1 (not retried)", asked.Load())
	}
	if hits.Load() != 0 {
		t.Fatalf("server hit %d times, want 0", hits.Load())
	}
}

func TestEmptyProviderTokenIsACredentialError(t *testing.T) {
	client := New("", "http://127.0.0.1:0", WithCredentialProvider(CredentialProviderFunc(func(context.Context) (string, error) {
		return "", nil
	})))
	if _, err := client.Health(t.Context()); !errors.Is(err, ErrNoCredential) {
		t.Fatalf("error %v, want ErrNoCredential", err)
	}
}

func TestES256ProviderAuthenticatesClientRequests(t *testing.T) {
	pemBytes, key := generatedKeyPEM(t, elliptic.P256())
	kid, err := JWKThumbprint(&key.PublicKey)
	if err != nil {
		t.Fatalf("thumbprint: %v", err)
	}
	p, err := NewES256AssertionProvider(pemBytes, "client-a", "tenant-a", kid, DefaultAssertionAudience)
	if err != nil {
		t.Fatalf("provider: %v", err)
	}
	var mu sync.Mutex
	var authorization, apiKey string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		authorization, apiKey = r.Header.Get("Authorization"), r.Header.Get("X-API-Key")
		mu.Unlock()
		w.Write([]byte(`{"status":"ok"}`))
	}))
	defer srv.Close()

	if _, err := New("", srv.URL, WithCredentialProvider(p)).Health(t.Context()); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	mu.Lock()
	defer mu.Unlock()
	if apiKey != "" {
		t.Fatalf("X-API-Key sent on the provider path: %q", apiKey)
	}
	token, ok := strings.CutPrefix(authorization, "Bearer ")
	if !ok {
		t.Fatalf("no bearer: %q", authorization)
	}
	verifyES256(t, token, &key.PublicKey)
	if got := decodeSegment(t, token, 0)["kid"]; got != kid {
		t.Fatalf("kid %v, want %s", got, kid)
	}
}

func TestGatewayAuthDenialKeepsItsReason(t *testing.T) {
	cases := []struct {
		body       string
		status     int
		sunset     bool
		assertion  bool
		wantReason string
	}{
		{`{"error":"unauthorized","reason":"legacy_key_sunset","message":"Register a client key or create a dashboard key"}`, 401, true, false, "legacy_key_sunset"},
		{`{"error":"unauthorized","reason":"asserted_skew","message":"iat outside the allowed clock skew"}`, 401, false, true, "asserted_skew"},
		{`{"error":"unauthorized","reason":"asserted_unregistered","message":"no registered key"}`, 401, false, true, "asserted_unregistered"},
		{`{"error":{"code":"unauthorized","reason":"legacy_key_sunset","message":"Register a client key"}}`, 401, true, false, "legacy_key_sunset"},
		{`{"error":"unauthorized","reason":"history_invalid","message":"client key registration is invalid"}`, 401, false, false, "history_invalid"},
		{`{"error":"unauthorized"}`, 401, false, false, ""},
	}
	for _, tc := range cases {
		t.Run(tc.wantReason, func(t *testing.T) {
			apiErr := parseAPIError([]byte(tc.body), tc.status)
			if apiErr.Code != "unauthorized" {
				t.Errorf("code %q, want unauthorized", apiErr.Code)
			}
			if apiErr.Reason != tc.wantReason {
				t.Errorf("reason %q, want %q", apiErr.Reason, tc.wantReason)
			}
			if apiErr.IsLegacyKeySunset() != tc.sunset {
				t.Errorf("IsLegacyKeySunset %t, want %t", apiErr.IsLegacyKeySunset(), tc.sunset)
			}
			if apiErr.IsAssertionDenied() != tc.assertion {
				t.Errorf("IsAssertionDenied %t, want %t", apiErr.IsAssertionDenied(), tc.assertion)
			}
		})
	}
}
