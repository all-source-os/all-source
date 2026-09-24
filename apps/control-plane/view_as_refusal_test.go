package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// refusalFakeCore captures IngestEvent calls so the test can assert the
// admin.viewas.write_refused alarm was written. It embeds clients.CoreClient so
// only the one method we exercise is implemented (the rest panic if hit).
type refusalFakeCore struct {
	clients.CoreClient
	ingested []clients.IngestEventRequest
}

func (f *refusalFakeCore) IngestEvent(_ context.Context, req clients.IngestEventRequest) (*clients.IngestEventResponse, error) {
	f.ingested = append(f.ingested, req)
	return &clients.IngestEventResponse{EventID: "evt-refused"}, nil
}

// buildRefusalRouter wires the EXACT production data-plane chain the refusal
// depends on: AuthMiddleware (validates the JWT + stashes auth_claims) →
// ViewAsWriteRefusal(alarm) → the route handler. A "write" handler flips a flag so
// the test can prove the mutation never executed. AuthMiddleware skips
// /api/v1/admin/*, so the route lives on a data-plane path (/api/v1/events) — the
// surface a leaked view_as token would actually be pointed at.
func buildRefusalRouter(t *testing.T, secret string, alarm viewAsAlarmFunc, wrote *bool) *gin.Engine {
	t.Helper()
	gin.SetMode(gin.TestMode)
	r := gin.New()
	r.Use(AuthMiddleware(NewAuthClient(secret, "")))
	r.Use(ViewAsWriteRefusal(alarm))

	write := func(c *gin.Context) {
		*wrote = true // if we ever get here on a view_as token, that's the breach
		c.JSON(http.StatusOK, gin.H{"ok": true})
	}
	read := func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"events": []any{}})
	}
	r.POST("/api/v1/events", write)
	r.PUT("/api/v1/events/:id", write)
	r.DELETE("/api/v1/events/:id", write)
	r.GET("/api/v1/events/query", read)
	return r
}

func doRefusalReq(t *testing.T, r *gin.Engine, method, path, token string) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequestWithContext(context.Background(), method, path, http.NoBody)
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	w := httptest.NewRecorder()
	r.ServeHTTP(w, req)
	return w
}

// TestViewAsWriteRefusal_RejectsAndAlarmsWrites is the core security assertion of
// Phase 7: a write request bearing a view_as token is REJECTED (no write executes)
// and the attempt is audited/alarmed (ADMIN_TENANT_POWER_TOOL §5.2 / §5.4).
func TestViewAsWriteRefusal_RejectsAndAlarmsWrites(t *testing.T) {
	const secret = "view-as-refusal-secret"
	auth := NewAuthClient(secret, "")

	// A real view_as token, minted exactly as production would (readonly + view_as,
	// distinct from any session), for admin "admin-7" viewing tenant "t-77".
	viewAsTok, err := auth.SignViewAsJWT("admin-7", "t-77")
	if err != nil {
		t.Fatalf("mint view-as token: %v", err)
	}

	for _, tc := range []struct {
		name   string
		method string
		path   string
	}{
		{"POST is refused", http.MethodPost, "/api/v1/events"},
		{"PUT is refused", http.MethodPut, "/api/v1/events/abc"},
		{"DELETE is refused", http.MethodDelete, "/api/v1/events/abc"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			core := &refusalFakeCore{}
			auditor := usecases.NewViewAsAuditor(core)
			wrote := false
			r := buildRefusalRouter(t, secret, auditor.RecordWriteRefused, &wrote)

			w := doRefusalReq(t, r, tc.method, tc.path, viewAsTok)

			// (1) Rejected with 403.
			if w.Code != http.StatusForbidden {
				t.Fatalf("%s: expected 403, got %d (%s)", tc.method, w.Code, w.Body.String())
			}
			// (2) The write NEVER executed — read-only by construction.
			if wrote {
				t.Fatalf("%s: write handler executed for a view_as token — BREACH", tc.method)
			}
			// (3) The attempt was alarmed as a durable Core event under admin-viewas.
			var alarm *clients.IngestEventRequest
			for i := range core.ingested {
				if core.ingested[i].EventType == usecases.ViewAsWriteRefusedEventType {
					alarm = &core.ingested[i]
					break
				}
			}
			if alarm == nil {
				t.Fatalf("%s: expected an admin.viewas.write_refused alarm event, got %+v", tc.method, core.ingested)
				return
			}
			if alarm.TenantID != usecases.ViewAsAuditTenant {
				t.Fatalf("alarm tenant: expected %q, got %q", usecases.ViewAsAuditTenant, alarm.TenantID)
			}
			if alarm.EntityID != "viewas:t-77" {
				t.Fatalf("alarm entity: expected viewas:t-77, got %q", alarm.EntityID)
			}
			if got, _ := alarm.Payload["act_as"].(string); got != "admin-7" { //nolint:errcheck // test plumbing
				t.Fatalf("alarm act_as: expected admin-7, got %q", got)
			}
			if got, _ := alarm.Payload["method"].(string); got != tc.method { //nolint:errcheck // test plumbing
				t.Fatalf("alarm method: expected %q, got %q", tc.method, got)
			}
		})
	}
}

// TestViewAsWriteRefusal_AllowsReads proves the refusal does NOT block reads — a
// view_as token on a GET passes through (reading the tenant's product is the whole
// point of view-as).
func TestViewAsWriteRefusal_AllowsReads(t *testing.T) {
	const secret = "view-as-refusal-secret"
	auth := NewAuthClient(secret, "")
	viewAsTok, err := auth.SignViewAsJWT("admin-7", "t-77")
	if err != nil {
		t.Fatalf("mint view-as token: %v", err)
	}

	core := &refusalFakeCore{}
	auditor := usecases.NewViewAsAuditor(core)
	wrote := false
	r := buildRefusalRouter(t, secret, auditor.RecordWriteRefused, &wrote)

	w := doRefusalReq(t, r, http.MethodGet, "/api/v1/events/query", viewAsTok)
	if w.Code != http.StatusOK {
		t.Fatalf("read with view_as token: expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	if len(core.ingested) != 0 {
		t.Fatalf("a read must not alarm; got %d events", len(core.ingested))
	}
}

// TestViewAsWriteRefusal_IgnoresNormalSessionWrites proves a NORMAL (non-view_as)
// token is untouched by the refusal — only view_as:true tokens are refused on
// writes. A developer-role session writes normally; the refusal is invisible to it.
func TestViewAsWriteRefusal_IgnoresNormalSessionWrites(t *testing.T) {
	const secret = "view-as-refusal-secret"
	auth := NewAuthClient(secret, "")

	// A normal session token (developer role, NO view_as) via the delegation signer.
	normalTok, err := auth.SignDelegationJWT("user-1", "t-77", entities.RoleDeveloper)
	if err != nil {
		t.Fatalf("mint normal token: %v", err)
	}

	core := &refusalFakeCore{}
	auditor := usecases.NewViewAsAuditor(core)
	wrote := false
	r := buildRefusalRouter(t, secret, auditor.RecordWriteRefused, &wrote)

	w := doRefusalReq(t, r, http.MethodPost, "/api/v1/events", normalTok)
	// The refusal passes it through; the write handler runs (200) and nothing is
	// alarmed. (This route has no RoleReadOnly gate — the point is ONLY that the
	// view_as refusal does not interfere with a normal session.)
	if w.Code != http.StatusOK {
		t.Fatalf("normal session write: expected 200 (refusal must ignore it), got %d (%s)", w.Code, w.Body.String())
	}
	if !wrote {
		t.Fatalf("normal session write should have executed")
	}
	if len(core.ingested) != 0 {
		t.Fatalf("a normal session write must not alarm; got %d events", len(core.ingested))
	}
}
