package allsource

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"math/big"
	"os"
	"reflect"
	"strings"
	"testing"
	"time"
)

// The gateway's asserted_keys_interop_test.go and the Rust SDK's
// es256_assertion.rs verify the same two fixtures, with the same client,
// tenant, audience and issue time.
const (
	interopKeyPath   = "../rust/tests/fixtures/es256_interop_test_key.pem"
	interopTokenPath = "../rust/tests/fixtures/es256_interop_token.jwt"
	interopClient    = "longhand-control-plane"
	interopTenant    = "asserted-tenant"
	interopIat       = int64(1_794_009_600)
)

func readFixture(t *testing.T, path string) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read fixture %s: %v", path, err)
	}
	return data
}

func decodeSegment(t *testing.T, token string, index int) map[string]any {
	t.Helper()
	parts := strings.Split(token, ".")
	if len(parts) != 3 {
		t.Fatalf("token has %d segments, want 3", len(parts))
	}
	raw, err := base64.RawURLEncoding.DecodeString(parts[index])
	if err != nil {
		t.Fatalf("segment %d is not base64url: %v", index, err)
	}
	var out map[string]any
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatalf("segment %d is not JSON: %v", index, err)
	}
	return out
}

func verifyES256(t *testing.T, token string, pub *ecdsa.PublicKey) {
	t.Helper()
	i := strings.LastIndexByte(token, '.')
	if i < 0 {
		t.Fatal("token has no signature segment")
	}
	sig, err := base64.RawURLEncoding.DecodeString(token[i+1:])
	if err != nil {
		t.Fatalf("signature is not base64url: %v", err)
	}
	if len(sig) != 64 {
		t.Fatalf("signature is %d bytes, want 64 (r||s)", len(sig))
	}
	digest := sha256.Sum256([]byte(token[:i]))
	r := new(big.Int).SetBytes(sig[:32])
	s := new(big.Int).SetBytes(sig[32:])
	if !ecdsa.Verify(pub, digest[:], r, s) {
		t.Fatal("signature does not verify with the public key")
	}
}

func withoutJTI(t *testing.T, claims map[string]any) map[string]any {
	t.Helper()
	jti, ok := claims["jti"].(string)
	if !ok || jti == "" {
		t.Fatalf("jti missing or empty: %v", claims["jti"])
	}
	out := make(map[string]any, len(claims))
	for k, v := range claims {
		if k != "jti" {
			out[k] = v
		}
	}
	return out
}

func TestES256InteropFixtureMatchesWhatTheProviderMints(t *testing.T) {
	pemBytes := readFixture(t, interopKeyPath)
	committed := strings.TrimSpace(string(readFixture(t, interopTokenPath)))

	key, err := parseP256PKCS8(pemBytes)
	if err != nil {
		t.Fatalf("fixture key: %v", err)
	}
	kid, err := JWKThumbprint(&key.PublicKey)
	if err != nil {
		t.Fatalf("thumbprint: %v", err)
	}

	verifyES256(t, committed, &key.PublicKey)
	if got := decodeSegment(t, committed, 0)["kid"]; got != kid {
		t.Fatalf("fixture kid %v, thumbprint of fixture key %s", got, kid)
	}

	p, err := NewES256AssertionProvider(pemBytes, interopClient, interopTenant, kid, DefaultAssertionAudience)
	if err != nil {
		t.Fatalf("provider: %v", err)
	}
	minted, err := p.MintAt(interopIat)
	if err != nil {
		t.Fatalf("mint: %v", err)
	}
	verifyES256(t, minted, &key.PublicKey)

	if a, b := decodeSegment(t, committed, 0), decodeSegment(t, minted, 0); !reflect.DeepEqual(a, b) {
		t.Fatalf("header drifted from the interop fixture:\n fixture %v\n minted  %v", a, b)
	}
	a := withoutJTI(t, decodeSegment(t, committed, 1))
	b := withoutJTI(t, decodeSegment(t, minted, 1))
	if !reflect.DeepEqual(a, b) {
		t.Fatalf("claims drifted from the interop fixture:\n fixture %v\n minted  %v", a, b)
	}
}

func generatedKeyPEM(t *testing.T, curve elliptic.Curve) ([]byte, *ecdsa.PrivateKey) {
	t.Helper()
	key, err := ecdsa.GenerateKey(curve, rand.Reader)
	if err != nil {
		t.Fatalf("generate key: %v", err)
	}
	der, err := x509.MarshalPKCS8PrivateKey(key)
	if err != nil {
		t.Fatalf("marshal pkcs8: %v", err)
	}
	return pem.EncodeToMemory(&pem.Block{Type: "PRIVATE KEY", Bytes: der}), key
}

func TestES256AssertionCarriesTheRegisteredIdentityAndA60sLifetime(t *testing.T) {
	pemBytes, key := generatedKeyPEM(t, elliptic.P256())
	p, err := NewES256AssertionProvider(pemBytes, "longhand-control-plane", "onboard-tenant", "kid-1", DefaultAssertionAudience)
	if err != nil {
		t.Fatalf("provider: %v", err)
	}

	token, err := p.Bearer(t.Context())
	if err != nil {
		t.Fatalf("bearer: %v", err)
	}
	verifyES256(t, token, &key.PublicKey)

	header := decodeSegment(t, token, 0)
	want := map[string]any{"alg": "ES256", "typ": "JWT", "kid": "kid-1"}
	if !reflect.DeepEqual(header, want) {
		t.Fatalf("header %v, want %v", header, want)
	}
	claims := decodeSegment(t, token, 1)
	if claims["iss"] != "longhand-control-plane" || claims["sub"] != "onboard-tenant" {
		t.Fatalf("iss/sub %v/%v", claims["iss"], claims["sub"])
	}
	if claims["aud"] != DefaultAssertionAudience {
		t.Fatalf("aud %v, want the single string %s", claims["aud"], DefaultAssertionAudience)
	}
	iat, _ := claims["iat"].(float64)
	exp, _ := claims["exp"].(float64)
	if exp-iat != 60 {
		t.Fatalf("exp-iat = %v, want 60", exp-iat)
	}
	withoutJTI(t, claims)
	if _, ok := claims["role"]; ok {
		t.Fatal("role must come from the registration, not the token")
	}
	if _, ok := claims["scope"]; ok {
		t.Fatal("scope must come from the registration, not the token")
	}
}

func TestES256AssertionIsReusedUntil15sBeforeExpiry(t *testing.T) {
	pemBytes, _ := generatedKeyPEM(t, elliptic.P256())
	p, err := NewES256AssertionProvider(pemBytes, "client", "tenant", "kid", DefaultAssertionAudience)
	if err != nil {
		t.Fatalf("provider: %v", err)
	}
	start := time.Unix(interopIat, 0)
	clock := start
	p.now = func() time.Time { return clock }

	first, err := p.Bearer(t.Context())
	if err != nil {
		t.Fatalf("bearer: %v", err)
	}
	clock = start.Add(44 * time.Second)
	if again, _ := p.Bearer(t.Context()); again != first {
		t.Fatal("assertion with 16s left was replaced; want it reused")
	}
	clock = start.Add(45 * time.Second)
	fresh, err := p.Bearer(t.Context())
	if err != nil {
		t.Fatalf("bearer: %v", err)
	}
	if fresh == first {
		t.Fatal("assertion with 15s left was reused; want a new one")
	}
	if iat := decodeSegment(t, fresh, 1)["iat"]; iat != float64(clock.Unix()) {
		t.Fatalf("fresh iat %v, want %d", iat, clock.Unix())
	}
}

func TestES256ProviderRefusesBadInputUpFront(t *testing.T) {
	p256PEM, p256Key := generatedKeyPEM(t, elliptic.P256())
	p384PEM, _ := generatedKeyPEM(t, elliptic.P384())
	sec1, err := x509.MarshalECPrivateKey(p256Key)
	if err != nil {
		t.Fatalf("marshal sec1: %v", err)
	}
	sec1PEM := pem.EncodeToMemory(&pem.Block{Type: "EC PRIVATE KEY", Bytes: sec1})

	cases := map[string]struct {
		pem  []byte
		args [4]string
	}{
		"garbage pem":    {[]byte("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n"), [4]string{"c", "t", "k", "a"}},
		"not pem":        {[]byte("not a key"), [4]string{"c", "t", "k", "a"}},
		"sec1 not pkcs8": {sec1PEM, [4]string{"c", "t", "k", "a"}},
		"p-384 key":      {p384PEM, [4]string{"c", "t", "k", "a"}},
		"empty client":   {p256PEM, [4]string{"", "t", "k", "a"}},
		"empty tenant":   {p256PEM, [4]string{"c", "", "k", "a"}},
		"empty kid":      {p256PEM, [4]string{"c", "t", "", "a"}},
		"empty audience": {p256PEM, [4]string{"c", "t", "k", ""}},
	}
	for name, tc := range cases {
		t.Run(name, func(t *testing.T) {
			if _, err := NewES256AssertionProvider(tc.pem, tc.args[0], tc.args[1], tc.args[2], tc.args[3]); err == nil {
				t.Fatal("expected an error")
			}
		})
	}
}

func TestJWKThumbprintRefusesNonP256(t *testing.T) {
	_, key := generatedKeyPEM(t, elliptic.P384())
	if _, err := JWKThumbprint(&key.PublicKey); err == nil {
		t.Fatal("expected an error for a P-384 key")
	}
	if _, err := JWKThumbprint(nil); err == nil {
		t.Fatal("expected an error for a nil key")
	}
}
