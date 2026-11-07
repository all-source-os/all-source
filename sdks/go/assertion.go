package allsource

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"crypto/x509"
	"encoding/base64"
	"encoding/json"
	"encoding/pem"
	"errors"
	"fmt"
	"sync"
	"time"
)

const (
	// AssertionLifetime is the lifetime of each assertion. The gateway refuses
	// anything longer than 60s.
	AssertionLifetime = 60 * time.Second
	// AssertionRefresh is how long before expiry a cached assertion is replaced.
	AssertionRefresh = 15 * time.Second
	// DefaultAssertionAudience is the gateway's default
	// ALLSOURCE_ASSERTION_AUDIENCE.
	DefaultAssertionAudience = "https://api.all-source.xyz"
)

type assertionHeader struct {
	Alg string `json:"alg"`
	Typ string `json:"typ"`
	Kid string `json:"kid"`
}

type assertionClaims struct {
	Iss string `json:"iss"`
	Sub string `json:"sub"`
	Aud string `json:"aud"`
	Iat int64  `json:"iat"`
	Exp int64  `json:"exp"`
	Jti string `json:"jti"`
}

// ES256AssertionProvider is a CredentialProvider that mints a 60-second ES256
// assertion signed with a P-256 private key, and reuses it until 15 seconds
// before it expires.
//
// The client id becomes iss, the tenant becomes sub, kid names the registered
// key (its RFC 7638 thumbprint, see JWKThumbprint) and the audience must equal
// the gateway's ALLSOURCE_ASSERTION_AUDIENCE. Role and scopes come from the
// key's registration, not from the token. Safe for concurrent use.
type ES256AssertionProvider struct {
	key      *ecdsa.PrivateKey
	clientID string
	tenant   string
	kid      string
	audience string
	now      func() time.Time

	mu    sync.Mutex
	token string
	exp   int64
}

// NewES256AssertionProvider builds a provider from a PKCS#8 PEM P-256 private
// key. A key that cannot sign ES256, or an empty clientID, tenant, kid or
// audience, is refused here rather than on the first request.
func NewES256AssertionProvider(pemPKCS8 []byte, clientID, tenant, kid, audience string) (*ES256AssertionProvider, error) {
	key, err := parseP256PKCS8(pemPKCS8)
	if err != nil {
		return nil, err
	}
	if clientID == "" || tenant == "" || kid == "" || audience == "" {
		return nil, errors.New("allsource: clientID, tenant, kid and audience are all required")
	}
	p := &ES256AssertionProvider{
		key:      key,
		clientID: clientID,
		tenant:   tenant,
		kid:      kid,
		audience: audience,
		now:      time.Now,
	}
	if _, err := p.MintAt(p.now().Unix()); err != nil {
		return nil, err
	}
	return p, nil
}

func parseP256PKCS8(pemPKCS8 []byte) (*ecdsa.PrivateKey, error) {
	block, _ := pem.Decode(pemPKCS8)
	if block == nil || block.Type != "PRIVATE KEY" {
		return nil, errors.New("allsource: client signing key is not a PKCS#8 PEM (\"PRIVATE KEY\" block)")
	}
	parsed, err := x509.ParsePKCS8PrivateKey(block.Bytes)
	if err != nil {
		return nil, fmt.Errorf("allsource: client signing key is not a P-256 PKCS#8 PEM: %w", err)
	}
	key, ok := parsed.(*ecdsa.PrivateKey)
	if !ok || key.Curve != elliptic.P256() {
		return nil, errors.New("allsource: client signing key is not a P-256 PKCS#8 PEM")
	}
	return key, nil
}

// Bearer returns the cached assertion, minting a new one when fewer than 15
// seconds of its lifetime remain.
func (p *ES256AssertionProvider) Bearer(_ context.Context) (string, error) {
	now := p.now().Unix()
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.token != "" && now+int64(AssertionRefresh/time.Second) < p.exp {
		return p.token, nil
	}
	token, err := p.MintAt(now)
	if err != nil {
		return "", err
	}
	p.token = token
	p.exp = now + int64(AssertionLifetime/time.Second)
	return token, nil
}

// MintAt mints an uncached assertion issued at iat (Unix seconds), for
// fixtures that pin the clock. Requests should go through Bearer.
func (p *ES256AssertionProvider) MintAt(iat int64) (string, error) {
	jti, err := newUUIDv4()
	if err != nil {
		return "", fmt.Errorf("allsource: client assertion jti: %w", err)
	}
	header, err := json.Marshal(assertionHeader{Alg: "ES256", Typ: "JWT", Kid: p.kid})
	if err != nil {
		return "", fmt.Errorf("allsource: client assertion header: %w", err)
	}
	claims, err := json.Marshal(assertionClaims{
		Iss: p.clientID,
		Sub: p.tenant,
		Aud: p.audience,
		Iat: iat,
		Exp: iat + int64(AssertionLifetime/time.Second),
		Jti: jti,
	})
	if err != nil {
		return "", fmt.Errorf("allsource: client assertion claims: %w", err)
	}
	b64 := base64.RawURLEncoding
	signingInput := b64.EncodeToString(header) + "." + b64.EncodeToString(claims)
	digest := sha256.Sum256([]byte(signingInput))
	r, s, err := ecdsa.Sign(rand.Reader, p.key, digest[:])
	if err != nil {
		return "", fmt.Errorf("allsource: client assertion signing failed: %w", err)
	}
	// JWS ES256 is the fixed-width r||s concatenation (RFC 7518 §3.4), not ASN.1.
	sig := make([]byte, 64)
	r.FillBytes(sig[:32])
	s.FillBytes(sig[32:])
	return signingInput + "." + b64.EncodeToString(sig), nil
}

// JWKThumbprint returns the RFC 7638 SHA-256 thumbprint of a P-256 public key,
// base64url without padding. The gateway expects it as the assertion's kid.
func JWKThumbprint(pub *ecdsa.PublicKey) (string, error) {
	if pub == nil || pub.Curve != elliptic.P256() {
		return "", errors.New("allsource: JWK thumbprint needs a P-256 public key")
	}
	point, err := pub.Bytes()
	if err != nil {
		return "", fmt.Errorf("allsource: JWK thumbprint: %w", err)
	}
	b64 := base64.RawURLEncoding
	// RFC 7638 §3.2: required members only, lexicographic order, no whitespace.
	canonical := `{"crv":"P-256","kty":"EC","x":"` + b64.EncodeToString(point[1:33]) +
		`","y":"` + b64.EncodeToString(point[33:65]) + `"}`
	sum := sha256.Sum256([]byte(canonical))
	return b64.EncodeToString(sum[:]), nil
}

func newUUIDv4() (string, error) {
	var b [16]byte
	if _, err := rand.Read(b[:]); err != nil {
		return "", err
	}
	b[6] = (b[6] & 0x0f) | 0x40
	b[8] = (b[8] & 0x3f) | 0x80
	return fmt.Sprintf("%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16]), nil
}
