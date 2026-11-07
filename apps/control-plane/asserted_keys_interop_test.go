package main

import (
	"crypto/ecdsa"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"net/http"
	"os"
	"strings"
	"testing"
	"time"
)

// The Rust SDK's tests/es256_assertion.rs mints the token fixture from the key
// fixture with this client, tenant, audience and issue time, and fails when what
// it mints today no longer matches the committed token.
const (
	interopFixtureDir = "../../sdks/rust/tests/fixtures/"
	interopIssuedAt   = 1794009600
)

func interopClientKey(t *testing.T) clientKeyFixture {
	t.Helper()
	raw, err := os.ReadFile(interopFixtureDir + "es256_interop_test_key.pem")
	if err != nil {
		t.Fatal(err)
	}
	block, _ := pem.Decode(raw)
	if block == nil {
		t.Fatal("interop test key is not PEM")
	}
	key, err := x509.ParsePKCS8PrivateKey(block.Bytes)
	if err != nil {
		t.Fatal(err)
	}
	private, ok := key.(*ecdsa.PrivateKey)
	if !ok {
		t.Fatalf("interop test key is %T, want P-256", key)
	}
	return clientKeyFixtureFor(private)
}

func interopToken(t *testing.T) string {
	t.Helper()
	raw, err := os.ReadFile(interopFixtureDir + "es256_interop_token.jwt")
	if err != nil {
		t.Fatal(err)
	}
	return strings.TrimSpace(string(raw))
}

func TestRustMintedAssertionIsAccepted(t *testing.T) {
	f := interopClientKey(t)
	token := interopToken(t)
	core := newFakeClientKeyCore(t, clientKeyRegisteredEvent(f, 1))
	client := NewAuthClient("synthetic-secret", core.server.URL)
	client.asserted.now = func() time.Time { return time.Unix(interopIssuedAt, 0) }

	recorder := assertedRequest(t, client, token, http.MethodGet, queryEventsPath, "")
	if recorder.Code != http.StatusOK {
		t.Fatalf("Rust-minted assertion refused: %d %s", recorder.Code, recorder.Body)
	}
	var got map[string]any
	if err := json.Unmarshal(recorder.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got["user"] != assertedClient+":"+f.kid || got["tenant"] != assertedTenant || got["jti"] == "" {
		t.Fatalf("auth context does not match the fixture identity: %v", got)
	}

	parts := strings.Split(token, ".")
	signature, err := base64.RawURLEncoding.DecodeString(parts[2])
	if err != nil {
		t.Fatal(err)
	}
	signature[len(signature)/2] ^= 1
	tampered := parts[0] + "." + parts[1] + "." + base64.RawURLEncoding.EncodeToString(signature)
	recorder = assertedRequest(t, client, tampered, http.MethodGet, queryEventsPath, "")
	if recorder.Code != http.StatusUnauthorized || assertedReason(t, recorder) != denialAssertedSignature {
		t.Fatalf("tampered signature: %d %s", recorder.Code, recorder.Body)
	}
}
