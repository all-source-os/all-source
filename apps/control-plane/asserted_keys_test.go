package main

import (
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"
)

const (
	assertedTenant = "asserted-tenant"
	assertedClient = "longhand-control-plane"
)

type clientKeyFixture struct {
	private *ecdsa.PrivateKey
	jwk     ecJWK
	kid     string
}

func newClientKeyFixture(t *testing.T) clientKeyFixture {
	t.Helper()
	private, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return clientKeyFixtureFor(private)
}

func clientKeyFixtureFor(private *ecdsa.PrivateKey) clientKeyFixture {
	jwk := ecJWK{Kty: "EC", Crv: "P-256",
		X: base64.RawURLEncoding.EncodeToString(private.X.FillBytes(make([]byte, 32))),
		Y: base64.RawURLEncoding.EncodeToString(private.Y.FillBytes(make([]byte, 32)))}
	return clientKeyFixture{private: private, jwk: jwk, kid: jwk.thumbprint()}
}

func (f clientKeyFixture) privateJWK() *ecJWK {
	raw, err := f.private.Bytes()
	if err != nil {
		panic("fixture key: " + err.Error())
	}
	private := f.jwk
	private.D = base64.RawURLEncoding.EncodeToString(raw)
	return &private
}

func clientKeyRegisteredEvent(f clientKeyFixture, version int, scopes ...string) map[string]any {
	if len(scopes) == 0 {
		scopes = []string{managedScopeRead, managedScopeWrite}
	}
	return map[string]any{"id": "registered-" + f.kid, "tenant_id": assertedTenant, "entity_id": clientKeyEntityPrefix + assertedTenant,
		"event_type": clientKeyRegistered, "timestamp": "2026-11-07T00:00:00Z", "version": version,
		"payload": map[string]any{"kid": f.kid, "client_id": assertedClient, "tenant_id": assertedTenant, "alg": "ES256",
			"jwk": map[string]any{"kty": "EC", "crv": "P-256", "x": f.jwk.X, "y": f.jwk.Y}, "role": "serviceaccount",
			"scopes": scopes, "active": true, "created_at": "2026-11-07T00:00:00Z", "created_by": "operator"}}
}

func clientKeyRevokedEvent(kid string, version int) map[string]any {
	return map[string]any{"id": "revoked-" + kid, "tenant_id": assertedTenant, "entity_id": clientKeyEntityPrefix + assertedTenant,
		"event_type": clientKeyRevoked, "timestamp": "2026-11-07T00:00:01Z", "version": version,
		"payload": map[string]any{"kid": kid, "revoked_at": "2026-11-07T00:00:01Z", "revoked_by": "operator"}}
}

func clientKeyPage(events ...map[string]any) map[string]any {
	return map[string]any{"events": events, "count": len(events), "total_count": len(events), "has_more": false, "entity_version": len(events),
		"archive_integrity": map[string]any{"protocol": "retained-entity-v1", "tenant_id": assertedTenant, "entity_id": clientKeyEntityPrefix + assertedTenant}}
}

type fakeClientKeyCore struct {
	mu       sync.Mutex
	page     map[string]any
	status   int
	reads    atomic.Int64
	ingested []map[string]any
	server   *httptest.Server
}

func newFakeClientKeyCore(t *testing.T, events ...map[string]any) *fakeClientKeyCore {
	t.Helper()
	core := &fakeClientKeyCore{page: clientKeyPage(events...), status: http.StatusOK}
	core.server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		core.mu.Lock()
		defer core.mu.Unlock()
		if r.Header.Get("Authorization") == "" {
			t.Error("Core read without internal authorization")
		}
		if r.Method == http.MethodPost {
			var event map[string]any
			if err := json.NewDecoder(r.Body).Decode(&event); err != nil {
				t.Error(err)
			}
			core.ingested = append(core.ingested, event)
			if _, err := w.Write([]byte(`{"event_id":"ack"}`)); err != nil {
				t.Error(err)
			}
			return
		}
		core.reads.Add(1)
		q := r.URL.Query()
		if q.Get("integrity") != "retained-entity-v1" || !strings.HasPrefix(q.Get("entity_id"), clientKeyEntityPrefix) {
			t.Errorf("client key read must be a strict retained read of clientkey:<tenant>: %s", r.URL.RawQuery)
		}
		if core.status != http.StatusOK {
			w.WriteHeader(core.status)
			return
		}
		if err := json.NewEncoder(w).Encode(core.page); err != nil {
			t.Error(err)
		}
	}))
	t.Cleanup(core.server.Close)
	return core
}

func (c *fakeClientKeyCore) set(status int, events ...map[string]any) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.status = status
	if events != nil {
		c.page = clientKeyPage(events...)
	}
}

func signAssertion(t *testing.T, f clientKeyFixture, edit func(claims jwt.MapClaims, header map[string]any)) string {
	t.Helper()
	now := time.Now()
	claims := jwt.MapClaims{"iss": assertedClient, "sub": assertedTenant, "aud": defaultAssertionAudience,
		"iat": now.Unix(), "exp": now.Add(time.Minute).Unix(), "jti": "jti-" + now.Format(time.RFC3339Nano)}
	token := jwt.NewWithClaims(jwt.SigningMethodES256, claims)
	token.Header["kid"] = f.kid
	if edit != nil {
		edit(claims, token.Header)
	}
	signed, err := token.SignedString(f.private)
	if err != nil {
		t.Fatal(err)
	}
	return signed
}

func assertedRequest(t *testing.T, client *AuthClient, token, method, path, body string) *httptest.ResponseRecorder {
	t.Helper()
	gin.SetMode(gin.TestMode)
	router := gin.New()
	router.Use(AuthMiddleware(client))
	router.Any("/*path", func(c *gin.Context) {
		auth, err := GetAuthContext(c)
		if err != nil {
			t.Error(err)
			return
		}
		c.JSON(http.StatusOK, gin.H{"user": auth.UserID, "tenant": auth.TenantID, "role": auth.Role, "api_key": auth.IsAPIKey, "jti": c.GetString("auth_assertion_jti")})
	})
	req := httptest.NewRequestWithContext(t.Context(), method, path, strings.NewReader(body))
	req.Header.Set("Authorization", "Bearer "+token)
	recorder := httptest.NewRecorder()
	router.ServeHTTP(recorder, req)
	return recorder
}

func assertedReason(t *testing.T, recorder *httptest.ResponseRecorder) string {
	t.Helper()
	var body struct {
		Reason string `json:"reason"`
	}
	if err := json.Unmarshal(recorder.Body.Bytes(), &body); err != nil {
		t.Fatalf("refusal body is not JSON: %s", recorder.Body)
	}
	return body.Reason
}

func TestClientKeyThumbprintIsRFC7638(t *testing.T) {
	f := newClientKeyFixture(t)
	canonical := `{"crv":"P-256","kty":"EC","x":"` + f.jwk.X + `","y":"` + f.jwk.Y + `"}`
	sum := sha256.Sum256([]byte(canonical))
	if f.kid != base64.RawURLEncoding.EncodeToString(sum[:]) {
		t.Fatal("kid is not the RFC 7638 thumbprint of the public JWK")
	}
}

func TestClientKeyJWKValidation(t *testing.T) {
	f := newClientKeyFixture(t)
	if _, err := f.jwk.publicKey(); err != nil {
		t.Fatal(err)
	}
	offCurve := f.jwk
	offCurve.Y = base64.RawURLEncoding.EncodeToString(make([]byte, 32))
	padded := f.jwk
	padded.X += "="
	short := f.jwk
	short.X = base64.RawURLEncoding.EncodeToString(make([]byte, 31))
	rsa := f.jwk
	rsa.Kty = "RSA"
	for name, jwk := range map[string]ecJWK{"off curve": offCurve, "padded": padded, "short": short, "kty": rsa} {
		if _, err := jwk.publicKey(); err == nil {
			t.Fatalf("%s JWK accepted", name)
		}
	}
	other := newClientKeyFixture(t)
	mismatched := f.privateJWK()
	mismatched.D = other.privateJWK().D
	if _, err := mismatched.privateKey(); err == nil {
		t.Fatal("private JWK whose d does not match x and y accepted")
	}
}

func TestPublicIngestRefusesClientKeyStream(t *testing.T) {
	for _, body := range []string{
		`{"entity_id":"clientkey:t","event_type":"x"}`,
		`{"entity_id":"other","event_type":"clientkey.registered"}`,
		`{"events":[{"entity_id":"a","event_type":"b"},{"entity_id":"clientkey:t","event_type":"b"}]}`,
		`[{"entity_id":"a","event_type":"clientkey.revoked"}]`,
	} {
		var value any
		if err := json.Unmarshal([]byte(body), &value); err != nil {
			t.Fatal(err)
		}
		if !containsKeyLifecycle(value) {
			t.Fatalf("public ingest would accept %s", body)
		}
	}
	var ordinary any
	if err := json.Unmarshal([]byte(`{"entity_id":"client:key","event_type":"client.keyed"}`), &ordinary); err != nil {
		t.Fatal(err)
	}
	if containsKeyLifecycle(ordinary) {
		t.Fatal("an ordinary event was refused")
	}
}

func TestAssertedKeyAuthenticatesFromTheRegistrationOnly(t *testing.T) {
	f := newClientKeyFixture(t)
	core := newFakeClientKeyCore(t, clientKeyRegisteredEvent(f, 1))
	client := NewAuthClient("synthetic-secret", core.server.URL)
	token := signAssertion(t, f, func(claims jwt.MapClaims, _ map[string]any) {
		claims["role"] = "admin"
		claims["scope"] = "schemas:write"
		claims["tenant_id"] = assertedTenant
	})
	recorder := assertedRequest(t, client, token, http.MethodGet, queryEventsPath, "")
	if recorder.Code != http.StatusOK {
		t.Fatalf("registered key refused: %d %s", recorder.Code, recorder.Body)
	}
	var got map[string]any
	if err := json.Unmarshal(recorder.Body.Bytes(), &got); err != nil {
		t.Fatal(err)
	}
	if got["user"] != assertedClient+":"+f.kid || got["tenant"] != assertedTenant || got["role"] != "serviceaccount" || got["api_key"] != true || got["jti"] == "" {
		t.Fatalf("auth context did not come from the registration: %v", got)
	}
	if recorder := assertedRequest(t, client, token, http.MethodPost, "/api/v1/schemas", "{}"); recorder.Code != http.StatusForbidden || assertedReason(t, recorder) != denialScope {
		t.Fatalf("a role claim expanded the registration: %d %s", recorder.Code, recorder.Body)
	}
}

func TestAssertedKeyRefusals(t *testing.T) {
	f := newClientKeyFixture(t)
	stranger := newClientKeyFixture(t)
	core := newFakeClientKeyCore(t, clientKeyRegisteredEvent(f, 1, managedScopeRead))
	now := time.Now()
	for _, tc := range []struct {
		name   string
		token  func() string
		status int
		reason string
	}{
		{"audience", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) { c["aud"] = "https://elsewhere" })
		}, 401, denialAssertedAudience},
		{"audience array", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) { c["aud"] = []string{defaultAssertionAudience} })
		}, 401, denialAssertedAudience},
		{"lifetime over 60s", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) { c["exp"] = now.Add(61 * time.Second).Unix() })
		}, 401, denialAssertedSkew},
		{"iat in the future", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) {
				c["iat"] = now.Add(40 * time.Second).Unix()
				c["exp"] = now.Add(90 * time.Second).Unix()
			})
		}, 401, denialAssertedSkew},
		{"nbf in the future", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) { c["nbf"] = now.Add(45 * time.Second).Unix() })
		}, 401, denialAssertedSkew},
		{"expired beyond skew", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) {
				c["iat"] = now.Add(-100 * time.Second).Unix()
				c["exp"] = now.Add(-40 * time.Second).Unix()
			})
		}, 401, denialAssertedExpired},
		{"no exp", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) { delete(c, "exp") })
		}, 401, denialAssertedExpired},
		{"no jti", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) { delete(c, "jti") })
		}, 401, denialAssertedSignature},
		{"issuer", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) { c["iss"] = "someone-else" })
		}, 401, denialAssertedUnregistered},
		{"tenant claim", func() string {
			return signAssertion(t, f, func(c jwt.MapClaims, _ map[string]any) { c["tenant_id"] = "another-tenant" })
		}, 401, denialAssertedUnregistered},
		{"unregistered kid", func() string { return signAssertion(t, stranger, nil) }, 401, denialAssertedUnregistered},
		{"signed by another key", func() string {
			return signAssertion(t, stranger, func(_ jwt.MapClaims, h map[string]any) { h["kid"] = f.kid })
		}, 401, denialAssertedSignature},
		{"HS256 body under an ES256 header", func() string {
			hmac, err := jwt.NewWithClaims(jwt.SigningMethodHS256, jwt.MapClaims{"iss": assertedClient, "sub": assertedTenant, "aud": defaultAssertionAudience,
				"iat": now.Unix(), "exp": now.Add(time.Minute).Unix(), "jti": "x"}).SignedString([]byte("synthetic-secret"))
			if err != nil {
				t.Fatal(err)
			}
			parts := strings.Split(hmac, ".")
			header := base64.RawURLEncoding.EncodeToString([]byte(`{"alg":"ES256","kid":"` + f.kid + `","typ":"JWT"}`))
			return header + "." + parts[1] + "." + parts[2]
		}, 401, denialAssertedSignature},
	} {
		t.Run(tc.name, func(t *testing.T) {
			client := NewAuthClient("synthetic-secret", core.server.URL)
			client.SetKeyAuthorityMode(keyAuthorityOff)
			before := denials(keyAuthorityEnforce, tc.reason, "unverified") + denials(keyAuthorityEnforce, tc.reason, assertedTenant)
			recorder := assertedRequest(t, client, tc.token(), http.MethodGet, queryEventsPath, "")
			if recorder.Code != tc.status || assertedReason(t, recorder) != tc.reason {
				t.Fatalf("got %d %s, want %d %s", recorder.Code, recorder.Body, tc.status, tc.reason)
			}
			after := denials(keyAuthorityEnforce, tc.reason, "unverified") + denials(keyAuthorityEnforce, tc.reason, assertedTenant)
			if after-before != 1 {
				t.Fatalf("denial counted %v times, want 1", after-before)
			}
		})
	}
}

func TestAssertedRoutingLeavesOtherTokensOnTheHMACPath(t *testing.T) {
	f := newClientKeyFixture(t)
	if _, routed := assertedHeader(signAssertion(t, f, func(_ jwt.MapClaims, h map[string]any) { h["typ"] = "at+jwt" })); routed {
		t.Fatal("a non-JWT typ was routed to the asserted path")
	}
	if _, routed := assertedHeader(signAssertion(t, f, func(_ jwt.MapClaims, h map[string]any) { delete(h, "kid") })); routed {
		t.Fatal("a token without kid was routed to the asserted path")
	}
	if _, routed := assertedHeader(signAssertion(t, f, func(_ jwt.MapClaims, h map[string]any) { delete(h, "typ") })); !routed {
		t.Fatal("an ES256 token with kid and no typ was not routed")
	}
	client := NewAuthClient("synthetic-secret", "")
	human := lifecycleClaims()
	human.IsAPIKey = false
	human.Role = "developer"
	if recorder := lifecycleRequest(t, client, human, http.MethodGet, queryEventsPath, ""); recorder.Code != http.StatusOK {
		t.Fatalf("HS256 session broke: %d", recorder.Code)
	}
}

func TestAssertedKeyRevocationAndOutageWindows(t *testing.T) {
	f := newClientKeyFixture(t)
	core := newFakeClientKeyCore(t, clientKeyRegisteredEvent(f, 1))
	client := NewAuthClient("synthetic-secret", core.server.URL)
	clock := time.Now()
	client.asserted.now = func() time.Time { return clock }
	request := func() *httptest.ResponseRecorder {
		return assertedRequest(t, client, signAssertion(t, f, nil), http.MethodGet, queryEventsPath, "")
	}
	if got := request().Code; got != http.StatusOK {
		t.Fatal(got)
	}
	if got := request().Code; got != http.StatusOK || core.reads.Load() != 1 {
		t.Fatalf("second request inside the TTL read Core again: %d reads", core.reads.Load())
	}

	core.set(http.StatusServiceUnavailable)
	clock = clock.Add(assertedKeyCacheTTL + time.Second)
	if got := request().Code; got != http.StatusOK {
		t.Fatalf("outage refused a key read %v ago: %d", assertedKeyCacheTTL, got)
	}
	clock = clock.Add(assertedKeyStaleLimit)
	recorder := request()
	if recorder.Code != http.StatusServiceUnavailable || recorder.Header().Get("Retry-After") != "5" || assertedReason(t, recorder) != denialAssertedUnavailable {
		t.Fatalf("outage past the stale limit: %d %s", recorder.Code, recorder.Body)
	}

	core.set(http.StatusOK, clientKeyRegisteredEvent(f, 1), clientKeyRevokedEvent(f.kid, 2))
	clock = clock.Add(keyAuthorityUnavailableBackoff + time.Second)
	recorder = request()
	if recorder.Code != http.StatusUnauthorized || assertedReason(t, recorder) != denialAssertedRevoked {
		t.Fatalf("revoked key: %d %s", recorder.Code, recorder.Body)
	}
	core.set(http.StatusServiceUnavailable)
	clock = clock.Add(assertedKeyCacheTTL + time.Second)
	if got := request().Code; got != http.StatusServiceUnavailable {
		t.Fatalf("a revoked key was served from the stale window: %d", got)
	}
}

func TestClientKeyFoldIsStrict(t *testing.T) {
	f := newClientKeyFixture(t)
	other := newClientKeyFixture(t)
	unknown := clientKeyRegisteredEvent(f, 2)
	unknown["event_type"] = "clientkey.rotated"
	unknown["id"] = "unknown"
	forged := clientKeyRegisteredEvent(other, 2)
	forged["id"] = "forged"
	payload, ok := forged["payload"].(map[string]any)
	if !ok {
		t.Fatal("fixture payload")
	}
	payload["kid"] = f.kid
	gap := clientKeyRegisteredEvent(other, 3)
	lateRegistration := clientKeyRegisteredEvent(f, 2)
	lateRegistration["timestamp"] = "2026-11-07T00:00:02Z"
	for name, tc := range map[string]struct {
		events []map[string]any
		reason string
	}{
		"unknown event type":          {[]map[string]any{clientKeyRegisteredEvent(f, 1), unknown}, denialHistory},
		"same kid, different JWK":     {[]map[string]any{clientKeyRegisteredEvent(f, 1), forged}, denialHistory},
		"version gap":                 {[]map[string]any{clientKeyRegisteredEvent(f, 1), gap}, denialHistory},
		"revoked before registration": {[]map[string]any{clientKeyRevokedEvent(f.kid, 1), lateRegistration}, denialAssertedRevoked},
	} {
		t.Run(name, func(t *testing.T) {
			core := newFakeClientKeyCore(t, tc.events...)
			if name == "version gap" {
				core.page["entity_version"] = 3
			}
			recorder := assertedRequest(t, NewAuthClient("synthetic-secret", core.server.URL), signAssertion(t, f, nil), http.MethodGet, queryEventsPath, "")
			if recorder.Code != http.StatusUnauthorized || assertedReason(t, recorder) != tc.reason {
				t.Fatalf("got %d %s, want 401 %s", recorder.Code, recorder.Body, tc.reason)
			}
		})
	}
}
