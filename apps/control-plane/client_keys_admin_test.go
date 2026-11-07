package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

	httphandlers "github.com/allsource/control-plane/internal/interfaces/http"
)

func clientKeyAdminRequest(t *testing.T, client *AuthClient, bearer, method, path, body string) *httptest.ResponseRecorder {
	t.Helper()
	gin.SetMode(gin.TestMode)
	router := gin.New()
	admin := router.Group("/api/v1/admin")
	admin.Use(httphandlers.AdminAuthMiddleware(client.jwtSecret))
	registerClientKeyRoutes(admin, client)
	req := httptest.NewRequestWithContext(t.Context(), method, path, strings.NewReader(body))
	req.Header.Set("Authorization", "Bearer "+bearer)
	recorder := httptest.NewRecorder()
	router.ServeHTTP(recorder, req)
	return recorder
}

func operatorToken(t *testing.T, client *AuthClient) string {
	t.Helper()
	token, err := jwt.NewWithClaims(jwt.SigningMethodHS256, jwt.MapClaims{"sub": "operator-1", "tenant_id": "system", "role": "admin",
		"iat": time.Now().Unix(), "exp": time.Now().Add(time.Hour).Unix()}).SignedString([]byte(client.jwtSecret))
	if err != nil {
		t.Fatal(err)
	}
	return token
}

func registrationBody(t *testing.T, jwk map[string]any, scopes ...string) string {
	t.Helper()
	body, err := json.Marshal(map[string]any{"client_id": assertedClient, "jwk": jwk, "scopes": scopes})
	if err != nil {
		t.Fatal(err)
	}
	return string(body)
}

func TestClientKeyRegistrationWritesTheRegisteredStream(t *testing.T) {
	f := newClientKeyFixture(t)
	core := newFakeClientKeyCore(t)
	client := NewAuthClient("synthetic-secret", core.server.URL)
	path := "/api/v1/admin/tenants/" + assertedTenant + "/client-keys"
	public := map[string]any{"kty": "EC", "crv": "P-256", "x": f.jwk.X, "y": f.jwk.Y}

	recorder := clientKeyAdminRequest(t, client, operatorToken(t, client), http.MethodPost, path, registrationBody(t, public, managedScopeRead))
	if recorder.Code != http.StatusCreated {
		t.Fatalf("registration: %d %s", recorder.Code, recorder.Body)
	}
	var created map[string]any
	if err := json.Unmarshal(recorder.Body.Bytes(), &created); err != nil || created["kid"] != f.kid {
		t.Fatalf("registration must return the server-computed thumbprint: %s", recorder.Body)
	}
	if len(core.ingested) != 1 {
		t.Fatalf("ingested %d events, want 1", len(core.ingested))
	}
	event := core.ingested[0]
	payload, ok := event["payload"].(map[string]any)
	if !ok || event["entity_id"] != clientKeyEntityPrefix+assertedTenant || event["event_type"] != clientKeyRegistered ||
		payload["kid"] != f.kid || payload["role"] != "serviceaccount" || payload["created_by"] != "operator-1" || payload["active"] != true {
		t.Fatalf("registered event shape: %v", event)
	}
	if jwk, ok := payload["jwk"].(map[string]any); !ok || jwk["d"] != nil {
		t.Fatalf("registered JWK: %v", payload["jwk"])
	}

	core.set(http.StatusOK, clientKeyRegisteredEvent(f, 1, managedScopeRead))
	if recorder := clientKeyAdminRequest(t, client, operatorToken(t, client), http.MethodGet, path, ""); recorder.Code != http.StatusOK ||
		!strings.Contains(recorder.Body.String(), f.kid) || strings.Contains(recorder.Body.String(), `"d"`) {
		t.Fatalf("list: %d %s", recorder.Code, recorder.Body)
	}
	if recorder := clientKeyAdminRequest(t, client, operatorToken(t, client), http.MethodPost, path, registrationBody(t, public, managedScopeRead)); recorder.Code != http.StatusOK || len(core.ingested) != 1 {
		t.Fatalf("identical re-registration should be idempotent: %d, %d writes", recorder.Code, len(core.ingested))
	}
	if recorder := clientKeyAdminRequest(t, client, operatorToken(t, client), http.MethodPost, path, registrationBody(t, public, managedScopeWrite)); recorder.Code != http.StatusConflict {
		t.Fatalf("re-registration with other scopes: %d", recorder.Code)
	}

	if recorder := clientKeyAdminRequest(t, client, operatorToken(t, client), http.MethodDelete, path+"/"+f.kid, ""); recorder.Code != http.StatusOK {
		t.Fatalf("revoke: %d %s", recorder.Code, recorder.Body)
	}
	if last := core.ingested[len(core.ingested)-1]; last["event_type"] != clientKeyRevoked {
		t.Fatalf("revoke wrote %v", last)
	}
	if recorder := clientKeyAdminRequest(t, client, operatorToken(t, client), http.MethodDelete, path+"/unknown-kid", ""); recorder.Code != http.StatusNotFound {
		t.Fatalf("revoke unknown kid: %d", recorder.Code)
	}
}

func TestClientKeyRegistrationRefusesBadInput(t *testing.T) {
	f := newClientKeyFixture(t)
	core := newFakeClientKeyCore(t)
	client := NewAuthClient("synthetic-secret", core.server.URL)
	path := "/api/v1/admin/tenants/" + assertedTenant + "/client-keys"
	public := map[string]any{"kty": "EC", "crv": "P-256", "x": f.jwk.X, "y": f.jwk.Y}
	withPrivate := map[string]any{"kty": "EC", "crv": "P-256", "x": f.jwk.X, "y": f.jwk.Y, "d": f.privateJWK().D}
	offCurve := map[string]any{"kty": "EC", "crv": "P-256", "x": f.jwk.X, "y": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}
	for name, body := range map[string]string{
		"private key":   registrationBody(t, withPrivate, managedScopeRead),
		"off curve":     registrationBody(t, offCurve, managedScopeRead),
		"unknown scope": registrationBody(t, public, "admin"),
		"no scopes":     registrationBody(t, public),
		"unknown field": `{"client_id":"c","jwk":{},"scopes":["events:read"],"role":"admin"}`,
	} {
		if recorder := clientKeyAdminRequest(t, client, operatorToken(t, client), http.MethodPost, path, body); recorder.Code != http.StatusBadRequest {
			t.Fatalf("%s: %d %s", name, recorder.Code, recorder.Body)
		}
	}
	if len(core.ingested) != 0 {
		t.Fatal("a refused registration reached Core")
	}

	managed, err := jwt.NewWithClaims(jwt.SigningMethodHS256, lifecycleClaims()).SignedString([]byte(client.jwtSecret))
	if err != nil {
		t.Fatal(err)
	}
	legacy, err := client.SignAPIKey(assertedTenant, "Longhand", "admin")
	if err != nil {
		t.Fatal(err)
	}
	for name, bearer := range map[string]string{"managed key": managed, "legacy key": legacy, "client assertion": signAssertion(t, f, nil)} {
		recorder := clientKeyAdminRequest(t, client, bearer, http.MethodPost, path, registrationBody(t, public, managedScopeRead))
		if recorder.Code != http.StatusForbidden && recorder.Code != http.StatusUnauthorized {
			t.Fatalf("%s registered a client key: %d", name, recorder.Code)
		}
	}
	if len(core.ingested) != 0 {
		t.Fatal("an API credential reached Core through the registration route")
	}
}
