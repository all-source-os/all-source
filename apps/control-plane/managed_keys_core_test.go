package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func TestManagedKeyActualCoreRevokeAcrossRestart(t *testing.T) {
	core := newWorkspaceCore(t)
	cp := core.controlPlane(t, core.url)
	workspaceJSON(t, cp.client, http.MethodPost, "/api/v1/tenants", map[string]any{
		"id": "proof-tenant", "name": "Synthetic lifecycle proof", "quota_preset": "unlimited",
	}, http.StatusCreated)
	var err error
	cp.delegation, err = newDelegationClient(core.url, core.url, core.url, cp.authClient.SignDelegationJWT, &http.Client{Timeout: 5 * time.Second})
	if err != nil {
		t.Fatal(err)
	}
	cp.queryMeter = newQueryUsageMeter(cp.persistQueryUsage)
	t.Cleanup(func() {
		if err := cp.queryMeter.close(); err != nil {
			t.Error(err)
		}
	})
	token, err := cp.authClient.IssueManagedAPIKey(t.Context(), "proof-tenant", "synthetic", entities.RoleServiceAccount)
	if err != nil {
		t.Fatal(err)
	}
	claims, err := cp.authClient.ValidateToken(token)
	if err != nil {
		t.Fatal(err)
	}
	router := gin.New()
	router.Use(AuthMiddleware(cp.authClient))
	router.POST("/api/v1/events", RequirePermission(entities.PermissionWrite), cp.ProxyIngestSingle)
	router.GET(queryEventsPath, RequirePermission(entities.PermissionRead), cp.ProxyEventsQuery)
	request := func(method, path, body string, status int) {
		t.Helper()
		req := httptest.NewRequestWithContext(t.Context(), method, path, strings.NewReader(body))
		req.Header.Set("Authorization", "Bearer "+token)
		req.Header.Set("Content-Type", "application/json")
		response := httptest.NewRecorder()
		router.ServeHTTP(response, req)
		if response.Code != status {
			t.Fatalf("%s %s: want%d got%d %s", method, path, status, response.Code, response.Body)
		}
		if status == 200 && method == http.MethodGet && !strings.Contains(response.Body.String(), "synthetic.received") {
			t.Fatal("missing durable application event")
		}
	}
	request(http.MethodPost, ingestEventsPath, `{"entity_id":"application-proof","event_type":"synthetic.received","payload":{"proof":true}}`, 200)
	request(http.MethodPost, ingestEventsPath, `{"entity_id":"apikey:proof-tenant","event_type":"apikey.created","payload":{}}`, 403)
	request(http.MethodGet, queryEventsPath+"?entity_id=application-proof", "", 200)
	workspaceJSON(t, cp.client, http.MethodPost, ingestEventsPath, map[string]any{
		"tenant_id": "proof-tenant", "entity_id": "apikey:proof-tenant", "event_type": "apikey.revoked", "payload": map[string]any{"key_id": claims.UserID},
	}, 200)
	request(http.MethodGet, queryEventsPath+"?entity_id=application-proof", "", 401)
	cp.queryMeter.flush(t.Context())
	core.restart(t)
	request(http.MethodGet, queryEventsPath+"?entity_id=application-proof", "", 401)
}
