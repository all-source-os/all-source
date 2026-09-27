package main

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http/httptest"
	"testing"
	"time"

	controlinternal "github.com/allsource/control-plane/internal"
	"github.com/allsource/control-plane/internal/application/usecases"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
	httphandlers "github.com/allsource/control-plane/internal/interfaces/http"
	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"
)

type partnershipHandlerCore struct {
	clients.CoreClient
	writes int
	fail   bool
}

func (f *partnershipHandlerCore) QueryEvents(context.Context, clients.QueryEventsRequest) (*clients.QueryEventsResponse, error) {
	if f.fail {
		return nil, context.DeadlineExceeded
	}
	return &clients.QueryEventsResponse{}, nil
}
func (f *partnershipHandlerCore) IngestEvent(context.Context, clients.IngestEventRequest) (*clients.IngestEventResponse, error) {
	f.writes++
	return &clients.IngestEventResponse{EventID: "synthetic"}, nil
}

func TestPartnershipHandlersRequireAdminAndPersistActor(t *testing.T) {
	gin.SetMode(gin.TestMode)
	core := &partnershipHandlerCore{}
	cp := &ControlPlane{container: &controlinternal.Container{PartnershipsUC: usecases.NewPartnershipsUseCase(core)}}
	router := gin.New()
	admin := router.Group("/api/v1/admin", httphandlers.AdminAuthMiddleware("test-partnership-secret"))
	admin.GET("/partnerships", cp.PartnershipsListHandler)
	admin.PUT("/partnerships", cp.PartnershipsSaveHandler)
	admin.GET("/partnerships/:id/history", cp.PartnershipsHistoryHandler)
	token := func(role entities.Role) string {
		t.Helper()
		claims := httphandlers.AdminClaims{UserID: "operator-1", Role: role, StandardClaims: jwt.StandardClaims{ExpiresAt: time.Now().Add(time.Hour).Unix()}}
		s, err := jwt.NewWithClaims(jwt.SigningMethodHS256, claims).SignedString([]byte("test-partnership-secret"))
		if err != nil {
			t.Fatal(err)
		}
		return s
	}
	body := []byte(`{"expected_revision":0,"actor":"spoofed","record":{"organization":"Example Partner","website":"https://example.com","kind":"vc","status":"research","messages":[],"sources":[]}}`)
	request := func(method, path, auth string, data []byte) *httptest.ResponseRecorder {
		t.Helper()
		r := httptest.NewRequest(method, path, bytes.NewReader(data))
		r.Header.Set("Content-Type", "application/json")
		if auth != "" {
			r.Header.Set("Authorization", "Bearer "+auth)
		}
		w := httptest.NewRecorder()
		router.ServeHTTP(w, r)
		return w
	}
	for _, route := range []struct{ method, path string }{{"GET", "/api/v1/admin/partnerships"}, {"PUT", "/api/v1/admin/partnerships"}, {"GET", "/api/v1/admin/partnerships/example.com/history"}} {
		if w := request(route.method, route.path, "", body); w.Code != 401 {
			t.Fatalf("missing auth: %d", w.Code)
		}
		if w := request(route.method, route.path, token(entities.RoleDeveloper), body); w.Code != 403 {
			t.Fatalf("tenant access: %d", w.Code)
		}
	}
	if core.writes != 0 {
		t.Fatal("unauthorised write")
	}
	w := request("PUT", "/api/v1/admin/partnerships", token(entities.RoleAdmin), body)
	if w.Code != 200 {
		t.Fatalf("save: %d %s", w.Code, w.Body.String())
	}
	var saved usecases.PartnershipRevision
	if err := json.Unmarshal(w.Body.Bytes(), &saved); err != nil {
		t.Fatal(err)
	}
	if saved.Actor != "operator-1" || core.writes != 1 {
		t.Fatalf("wrong actor/write count: %#v", saved)
	}
	if w.Header().Get("Cache-Control") != "private, no-store" {
		t.Fatal("private response could be cached")
	}
	core.fail = true
	w = request("GET", "/api/v1/admin/partnerships", token(entities.RoleAdmin), nil)
	if w.Code != 503 || bytes.Contains(w.Body.Bytes(), []byte("deadline")) {
		t.Fatalf("failure leaked or looked empty: %d %s", w.Code, w.Body.String())
	}
}
