package main

import (
	"io"
	"net/http"
	"strings"
	"testing"

	"github.com/go-resty/resty/v2"
)

type providerFixtureTransport func(*http.Request) (*http.Response, error)

func (f providerFixtureTransport) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }

func TestOAuthProviderVerifiedEmail(t *testing.T) {
	for _, tc := range []struct {
		name, provider, profile, emails, wantEmail string
		verified, denied                           bool
	}{
		{"github-public-email-is-not-proof", "github", `{"id":123,"email":"spoof@example.test"}`, `[{"email":"real@example.test","primary":true,"verified":true}]`, "real@example.test", true, false},
		{"github-unverified-denied", "github", `{"id":123,"email":"spoof@example.test"}`, `[{"email":"spoof@example.test","primary":true,"verified":false}]`, "", false, true},
		{"github-secondary-verified", "github", `{"id":123}`, `[{"email":"wrong@example.test","primary":true,"verified":false},{"email":"real@example.test","primary":false,"verified":true}]`, "real@example.test", true, false},
		{"google-verified", "google", `{"id":"123","email":"real@example.test","verified_email":true}`, "", "real@example.test", true, false},
		{"google-missing-proof", "google", `{"id":"123","email":"real@example.test"}`, "", "real@example.test", false, false},
		{"google-string-is-not-proof", "google", `{"id":"123","email":"real@example.test","verified_email":"true"}`, "", "real@example.test", false, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			client := resty.New().SetTransport(providerFixtureTransport(func(r *http.Request) (*http.Response, error) {
				body := tc.profile
				if r.URL.String() == githubEmailsURL {
					body = tc.emails
				}
				return &http.Response{StatusCode: 200, Header: http.Header{"Content-Type": []string{"application/json"}}, Body: io.NopCloser(strings.NewReader(body)), Request: r}, nil
			}))
			identity, err := fetchUserInfo(client, tc.provider, "synthetic-provider-token")
			if tc.denied {
				if err == nil {
					t.Fatal("unverified provider email accepted")
				}
				return
			}
			if err != nil || identity.Email != tc.wantEmail || identity.EmailVerified != tc.verified {
				t.Fatal("incorrect provider email verification")
			}
		})
	}
}
