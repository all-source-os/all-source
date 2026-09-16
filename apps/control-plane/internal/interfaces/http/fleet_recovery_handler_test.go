package http

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/dgrijalva/jwt-go"
	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
	"github.com/allsource/control-plane/internal/domain/entities"
	"github.com/allsource/control-plane/internal/infrastructure/clients"
	"github.com/allsource/control-plane/internal/infrastructure/persistence"
)

// --- mock Core client for fleet/recovery tests ---

// fleetMockCore implements just the CoreClient methods the fleet + recovery use
// cases touch; the embedded interface makes the rest compile (and panic if hit).
type fleetMockCore struct {
	clients.CoreClient

	// per-tenant event counts (drives GetTenantStats + the data-tenant heuristic)
	statsByTenant map[string]int64
	// per-tenant newest event timestamp returned by the unfiltered desc-probe
	newestByTenant map[string]time.Time
	// per-tenant newest SYNC event timestamp (event_type=prime.); honored only
	// when the probe carries a sync prefix, mirroring Core's server-side filter
	newestSyncByTenant map[string]time.Time
	// recorded ingest events (for audit assertions)
	ingested []clients.IngestEventRequest
	// health response
	health *clients.HealthResponse
	// existing API keys per tenant (for rotate-keys)
	keysByTenant map[string][]clients.CoreAPIKeyInfo
	revoked      []string
	created      []clients.CreateCoreAPIKeyRequest
	// tenants returned by ListTenants (for reap-demo); deletedTenants records
	// the ids passed to DeleteTenant.
	tenants        []clients.TenantResponse
	deletedTenants []string
}

func newFleetMockCore() *fleetMockCore {
	return &fleetMockCore{
		statsByTenant:      map[string]int64{},
		newestByTenant:     map[string]time.Time{},
		newestSyncByTenant: map[string]time.Time{},
		keysByTenant:       map[string][]clients.CoreAPIKeyInfo{},
		health:             &clients.HealthResponse{Status: "healthy"},
	}
}

func (m *fleetMockCore) GetTenantStats(_ context.Context, tenantID string) (*clients.TenantStatsResponse, error) {
	return &clients.TenantStatsResponse{TenantID: tenantID, EventCount: m.statsByTenant[tenantID]}, nil
}

func (m *fleetMockCore) QueryEvents(_ context.Context, req clients.QueryEventsRequest) (*clients.QueryEventsResponse, error) {
	// Mirror Core's server-side event_type prefix filter: a probe with a sync
	// prefix (prime.) only returns sync events for this tenant.
	var ts time.Time
	if req.EventType != "" {
		ts = m.newestSyncByTenant[req.TenantID]
	} else {
		ts = m.newestByTenant[req.TenantID]
	}
	if ts.IsZero() {
		return &clients.QueryEventsResponse{Events: nil, Count: 0}, nil
	}
	return &clients.QueryEventsResponse{
		Events: []clients.EventEntry{{ID: "e1", EntityID: req.TenantID, Timestamp: ts.UTC().Format(time.RFC3339Nano)}},
		Count:  1,
	}, nil
}

func (m *fleetMockCore) IngestEvent(_ context.Context, req clients.IngestEventRequest) (*clients.IngestEventResponse, error) {
	m.ingested = append(m.ingested, req)
	return &clients.IngestEventResponse{EventID: "evt-001"}, nil
}

func (m *fleetMockCore) HealthCheck(_ context.Context) (*clients.HealthResponse, error) {
	return m.health, nil
}

func (m *fleetMockCore) ListCoreAPIKeys(_ context.Context, tenantID string) ([]clients.CoreAPIKeyInfo, error) {
	return m.keysByTenant[tenantID], nil
}

func (m *fleetMockCore) CreateCoreAPIKey(_ context.Context, req clients.CreateCoreAPIKeyRequest) (*clients.CreateCoreAPIKeyResponse, error) {
	m.created = append(m.created, req)
	return &clients.CreateCoreAPIKeyResponse{ID: "newkey-1", Key: "ask_newkey"}, nil
}

func (m *fleetMockCore) RevokeAPIKey(_ context.Context, keyID string) error {
	m.revoked = append(m.revoked, keyID)
	return nil
}

func (m *fleetMockCore) ListTenants(_ context.Context) (*clients.ListTenantsResponse, error) {
	return &clients.ListTenantsResponse{Tenants: m.tenants, Total: len(m.tenants)}, nil
}

func (m *fleetMockCore) DeleteTenant(_ context.Context, tenantID string) error {
	m.deletedTenants = append(m.deletedTenants, tenantID)
	return nil
}

// recoveryAudits returns only the admin.recovery.* events recorded under the
// admin-recovery system tenant.
func (m *fleetMockCore) recoveryAudits() []clients.IngestEventRequest {
	out := []clients.IngestEventRequest{}
	for _, e := range m.ingested {
		if e.TenantID == usecases.RecoveryAuditTenant {
			out = append(out, e)
		}
	}
	return out
}

// --- test harness ---

type fleetTestEnv struct {
	repo    *persistence.MemoryTenantRepository
	core    *fleetMockCore
	fleetUC *usecases.FleetHealthUseCase
	recUC   *usecases.RecoveryUseCase
	fleetH  *FleetHealthHandler
	recH    *RecoveryHandler
}

const fleetTestJWTSecret = "fleet-test-secret"

func newFleetTestEnv(t *testing.T, queryHealthURL string) *fleetTestEnv {
	t.Helper()
	repo := persistence.NewMemoryTenantRepository()
	core := newFleetMockCore()
	auditRepo := persistence.NewMemoryAuditRepository()

	fleetUC := usecases.NewFleetHealthUseCase(usecases.FleetHealthConfig{
		TenantRepo:     repo,
		CoreClient:     core,
		QueryHealthURL: queryHealthURL,
	})
	updateSubsUC := usecases.NewUpdateSubscriptionMetadataUseCase(repo, auditRepo)
	auditor := usecases.NewRecoveryAuditor(core)
	recUC := usecases.NewRecoveryUseCase(usecases.RecoveryDeps{
		TenantRepo:   repo,
		CoreClient:   core,
		UpdateSubsUC: updateSubsUC,
		StartReplay:  nil, // restore audit-only path in tests
		Auditor:      auditor,
		JWTSecret:    fleetTestJWTSecret,
	})
	return &fleetTestEnv{
		repo:    repo,
		core:    core,
		fleetUC: fleetUC,
		recUC:   recUC,
		fleetH:  NewFleetHealthHandler(fleetUC),
		recH:    NewRecoveryHandler(recUC),
	}
}

// router builds the real admin router with AdminAuthMiddleware so 403 behavior
// is exercised end-to-end (proves middleware reuse).
func (e *fleetTestEnv) router() *gin.Engine {
	gin.SetMode(gin.TestMode)
	r := gin.New()
	admin := r.Group("/api/v1/admin")
	admin.Use(AdminAuthMiddleware(fleetTestJWTSecret))
	admin.POST("/tenants/reap-demo", e.recH.ReapDemo)
	admin.GET("/fleet/health", e.fleetH.GetFleetHealth)
	admin.GET("/fleet/health/:id", e.fleetH.GetTenantHealth)
	rec := admin.Group("/recovery")
	rec.GET("/diagnose/edition", e.fleetH.DiagnoseEdition)
	rec.GET("/:id/diagnose-identity", e.fleetH.DiagnoseIdentity)
	rec.POST("/:id/resync", e.recH.Resync)
	rec.POST("/:id/reconcile-subscription", e.recH.ReconcileSubscription)
	rec.POST("/:id/resolve-dunning", e.recH.ResolveDunning)
	rec.POST("/:id/rotate-keys", e.recH.RotateKeys)
	rec.POST("/:id/reprovision", e.recH.Reprovision)
	rec.POST("/:id/restore", e.recH.Restore)
	rec.POST("/batch", e.recH.Batch)
	return r
}

func adminToken(t *testing.T, role entities.Role) string {
	t.Helper()
	claims := &AdminClaims{
		UserID: "admin-1", Username: "admin", Email: "admin@all-source.xyz",
		TenantID: "system", Role: role,
		StandardClaims: jwt.StandardClaims{ExpiresAt: time.Now().Add(time.Hour).Unix(), IssuedAt: time.Now().Unix()},
	}
	tok := jwt.NewWithClaims(jwt.SigningMethodHS256, claims)
	s, err := tok.SignedString([]byte(fleetTestJWTSecret))
	if err != nil {
		t.Fatalf("sign token: %v", err)
	}
	return s
}

func doAdmin(t *testing.T, r *gin.Engine, method, path, token string, body any) *httptest.ResponseRecorder {
	t.Helper()
	var buf bytes.Buffer
	if body != nil {
		if err := json.NewEncoder(&buf).Encode(body); err != nil {
			t.Fatalf("encode body: %v", err)
		}
	}
	req := httptest.NewRequestWithContext(context.Background(), method, path, &buf)
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	w := httptest.NewRecorder()
	r.ServeHTTP(w, req)
	return w
}

func seed(t *testing.T, repo *persistence.MemoryTenantRepository, tenants ...*entities.Tenant) {
	t.Helper()
	for _, tn := range tenants {
		if err := repo.Save(tn); err != nil {
			t.Fatalf("seed: %v", err)
		}
	}
}

func mustActiveTenant(id, name string, meta map[string]interface{}) *entities.Tenant {
	return &entities.Tenant{ID: id, Name: name, Status: entities.TenantStatusActive, CreatedAt: time.Now(), UpdatedAt: time.Now(), Metadata: meta}
}

// ============================================================================
// P0 acceptance
// ============================================================================

// GET /fleet/health returns the four counts + worst-N with per-tenant reasons,
// and a tenant with EventsUsed >= EventsQuota surfaces events_quota_pct At-Risk.
func TestFleetHealth_CountsAndQuotaAtRisk(t *testing.T) {
	env := newFleetTestEnv(t, "")
	// Healthy tenant.
	seed(t, env.repo, mustActiveTenant("t-ok", "OK Co", map[string]interface{}{
		"subscription": map[string]interface{}{"tier": "indie", "status": "active"},
		"quotas":       map[string]interface{}{"events_used": float64(100), "events_quota": float64(500000)},
	}))
	// Over-quota tenant (used >= quota) → events_quota_pct At-Risk.
	seed(t, env.repo, mustActiveTenant("t-overquota", "Over Quota", map[string]interface{}{
		"subscription": map[string]interface{}{"tier": "indie", "status": "active"},
		"quotas":       map[string]interface{}{"events_used": float64(600000), "events_quota": float64(500000)},
	}))
	env.core.statsByTenant["t-ok"] = 100
	env.core.statsByTenant["t-overquota"] = 600000
	env.core.newestByTenant["t-ok"] = time.Now().Add(-1 * time.Hour)
	env.core.newestByTenant["t-overquota"] = time.Now().Add(-1 * time.Hour)

	r := env.router()
	w := doAdmin(t, r, http.MethodGet, "/api/v1/admin/fleet/health", adminToken(t, entities.RoleAdmin), nil)
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d (%s)", w.Code, w.Body.String())
	}

	var resp struct {
		Summary usecases.FleetSummary  `json:"summary"`
		Worst   []usecases.WorstTenant `json:"worst"`
		Links   map[string]any         `json:"_links"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if resp.Summary.Total != 2 {
		t.Errorf("total = %d, want 2", resp.Summary.Total)
	}
	if resp.Summary.AtRisk != 1 {
		t.Errorf("at_risk = %d, want 1", resp.Summary.AtRisk)
	}
	if resp.Summary.Healthy != 1 {
		t.Errorf("healthy = %d, want 1", resp.Summary.Healthy)
	}
	if len(resp.Links) == 0 {
		t.Error("expected _links")
	}
	// worst-N should carry the over-quota tenant with an events_quota_pct reason.
	var found bool
	for _, wt := range resp.Worst {
		if wt.TenantID == "t-overquota" {
			if wt.Tier != "at_risk" {
				t.Errorf("over-quota tier = %s, want at_risk", wt.Tier)
			}
			for _, reason := range wt.Reasons {
				if reason.Name == "events_quota_pct" && reason.Tier == "at_risk" {
					found = true
				}
			}
		}
	}
	if !found {
		t.Errorf("expected events_quota_pct at_risk reason for t-overquota; worst=%+v", resp.Worst)
	}
}

// GET /fleet/health/:id returns the full single-tenant assessment (all 11 signals).
func TestTenantHealth_AllSignals(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("t-1", "Tenant One", map[string]interface{}{
		"subscription": map[string]interface{}{"tier": "studio", "status": "active"},
		"quotas":       map[string]interface{}{"events_used": float64(10), "events_quota": float64(5000000)},
	}))
	env.core.statsByTenant["t-1"] = 10

	r := env.router()
	w := doAdmin(t, r, http.MethodGet, "/api/v1/admin/fleet/health/t-1", adminToken(t, entities.RoleAdmin), nil)
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	var resp struct {
		Signals     []map[string]any `json:"signals"`
		Tier        string           `json:"tier"`
		BillingTier string           `json:"billing_tier"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if len(resp.Signals) != 10 {
		t.Errorf("expected 10 enumerated signals, got %d", len(resp.Signals))
	}
	if resp.BillingTier != "studio" {
		t.Errorf("billing_tier = %s, want studio", resp.BillingTier)
	}
}

// Edition trap: QS edition=community + >=2 non-community data tenants flags the
// trap and returns the ALLSOURCE_EDITION=enterprise command.
func TestDiagnoseEdition_TrapDetected(t *testing.T) {
	// Stand up a fake QS /health that reports edition=community.
	qs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_ = json.NewEncoder(w).Encode(map[string]any{"status": "healthy", "edition": "community"}) //nolint:errcheck // test response
	}))
	defer qs.Close()

	env := newFleetTestEnv(t, qs.URL)
	// Two non-community tenants WITH data.
	seed(t, env.repo, mustActiveTenant("acme", "Acme", nil), mustActiveTenant("globex", "Globex", nil))
	env.core.statsByTenant["acme"] = 5000
	env.core.statsByTenant["globex"] = 9000

	r := env.router()
	w := doAdmin(t, r, http.MethodGet, "/api/v1/admin/recovery/diagnose/edition", adminToken(t, entities.RoleAdmin), nil)
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	var resp struct {
		Diagnosis usecases.EditionDiagnosis `json:"diagnosis"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if !resp.Diagnosis.TrapDetected {
		t.Errorf("expected trap_detected=true; got %+v", resp.Diagnosis)
	}
	if resp.Diagnosis.QSEdition != "community" {
		t.Errorf("qs_edition = %s, want community", resp.Diagnosis.QSEdition)
	}
	if want := "ALLSOURCE_EDITION=enterprise"; !bytes.Contains([]byte(resp.Diagnosis.RemediationCommand), []byte(want)) {
		t.Errorf("remediation_command = %q, want it to contain %q", resp.Diagnosis.RemediationCommand, want)
	}
}

// Every fleet/recovery endpoint returns 403 without an admin JWT (proves
// AdminAuthMiddleware reuse).
func TestFleetRecovery_403WithoutAdmin(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("t-1", "T1", nil))
	r := env.router()

	cases := []struct {
		method, path string
	}{
		{http.MethodGet, "/api/v1/admin/fleet/health"},
		{http.MethodGet, "/api/v1/admin/fleet/health/t-1"},
		{http.MethodGet, "/api/v1/admin/recovery/diagnose/edition"},
		{http.MethodGet, "/api/v1/admin/recovery/t-1/diagnose-identity"},
		{http.MethodPost, "/api/v1/admin/recovery/t-1/resync"},
		{http.MethodPost, "/api/v1/admin/recovery/t-1/reprovision"},
		{http.MethodPost, "/api/v1/admin/recovery/t-1/rotate-keys"},
		{http.MethodPost, "/api/v1/admin/recovery/t-1/restore"},
		{http.MethodPost, "/api/v1/admin/recovery/batch"},
	}
	for _, tc := range cases {
		t.Run(tc.method+"_"+tc.path, func(t *testing.T) {
			// No token at all → 401.
			w := doAdmin(t, r, tc.method, tc.path, "", map[string]any{})
			if w.Code != http.StatusUnauthorized {
				t.Errorf("no token: expected 401, got %d", w.Code)
			}
			// Non-admin role → 403.
			w = doAdmin(t, r, tc.method, tc.path, adminToken(t, entities.RoleDeveloper), map[string]any{})
			if w.Code != http.StatusForbidden {
				t.Errorf("developer role: expected 403, got %d (%s)", w.Code, w.Body.String())
			}
		})
	}
}

// ============================================================================
// P1 acceptance
// ============================================================================

// Every mutating endpoint with ?dry_run=true returns a `would` preview, makes no
// state change, and (for token-gated actions) returns a confirm_token.
func TestRecovery_DryRunPreviewNoMutation(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("t-dr", "DryRun Co", map[string]interface{}{
		"subscription": map[string]interface{}{"tier": "indie", "status": "active"},
	}))
	env.core.keysByTenant["t-dr"] = []clients.CoreAPIKeyInfo{{ID: "k1", Name: "main", TenantID: "t-dr", Active: true}}
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	// rotate-keys is token-gated → dry-run must return a confirm_token.
	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-dr/rotate-keys?dry_run=true", tok, map[string]any{"reason": "rotate"})
	if w.Code != http.StatusOK {
		t.Fatalf("rotate dry-run: expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	var rr usecases.RecoveryResult
	if err := json.Unmarshal(w.Body.Bytes(), &rr); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if !rr.DryRun || rr.Would == nil {
		t.Errorf("expected dry_run + would preview, got %+v", rr)
	}
	if rr.ConfirmToken == "" {
		t.Errorf("rotate-keys dry-run must return a confirm_token")
	}
	if len(env.core.revoked) != 0 || len(env.core.created) != 0 {
		t.Errorf("dry-run must not rotate keys (revoked=%d created=%d)", len(env.core.revoked), len(env.core.created))
	}

	// resync (Guarded, not token-gated): dry-run returns a preview, no marker event.
	beforeIngest := len(env.core.ingested)
	w = doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-dr/resync?dry_run=true", tok, map[string]any{"reason": "x"})
	if w.Code != http.StatusOK {
		t.Fatalf("resync dry-run: expected 200, got %d", w.Code)
	}
	// only a preview audit may have been written; no tenant.resync.requested marker
	for _, e := range env.core.ingested[beforeIngest:] {
		if e.EventType == "tenant.resync.requested" {
			t.Errorf("resync dry-run must not write the resync marker event")
		}
	}
}

// reprovision without a matching confirm_tenant_id → 400, no-op.
func TestReprovision_RequiresConfirmTenantID(t *testing.T) {
	env := newFleetTestEnv(t, "")
	// suspended so the active+recent hard-reject doesn't fire first
	tn := mustActiveTenant("t-rp", "Reprov", map[string]interface{}{
		"subscription": map[string]interface{}{"tier": "indie", "status": "active"},
	})
	tn.Status = entities.TenantStatusSuspended
	seed(t, env.repo, tn)
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	// apply without confirm_tenant_id → 400
	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-rp/reprovision", tok, map[string]any{"reason": "fix"})
	if w.Code != http.StatusBadRequest {
		t.Fatalf("expected 400 without confirm_tenant_id, got %d (%s)", w.Code, w.Body.String())
	}
	// wrong confirm_tenant_id → 400
	w = doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-rp/reprovision", tok, map[string]any{"reason": "fix", "confirm_tenant_id": "WRONG"})
	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400 with wrong confirm_tenant_id, got %d", w.Code)
	}
	// no recovery audit should have been written for the rejected attempts
	if n := len(env.core.recoveryAudits()); n != 0 {
		t.Errorf("rejected reprovision must not write a recovery audit, got %d", n)
	}
}

// reprovision on an active tenant with recent events is hard-rejected (409).
func TestReprovision_HardRejectActiveRecent(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("t-live", "Live", map[string]interface{}{
		"subscription": map[string]interface{}{"tier": "studio", "status": "active"},
	}))
	env.core.statsByTenant["t-live"] = 100
	env.core.newestByTenant["t-live"] = time.Now().Add(-1 * time.Hour) // ingested in last 24h
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	// even WITH a matching confirm_tenant_id, an active+recent tenant is rejected.
	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-live/reprovision", tok, map[string]any{
		"reason": "fix", "confirm_tenant_id": "t-live",
	})
	if w.Code != http.StatusConflict {
		t.Fatalf("expected 409 for active+recent reprovision, got %d (%s)", w.Code, w.Body.String())
	}
}

// batch with > 100 filtered tenants is capped/rejected.
func TestBatch_RejectsOverCeiling(t *testing.T) {
	env := newFleetTestEnv(t, "")
	// Seed 101 tenants all matching the filter (status=active).
	for i := 0; i < 101; i++ {
		seed(t, env.repo, mustActiveTenant("batch-"+itoaTest(i), "B", nil))
	}
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/batch?dry_run=true", tok, map[string]any{
		"reason": "backfill",
		"action": "reconcile_subscription",
		"filter": map[string]any{"status": "active"},
	})
	if w.Code != http.StatusUnprocessableEntity {
		t.Fatalf("expected 422 for >100 matched, got %d (%s)", w.Code, w.Body.String())
	}
}

// batch forbids Destructive sub-actions.
func TestBatch_ForbidsDestructiveSubAction(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("b1", "B1", nil))
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/batch?dry_run=true", tok, map[string]any{
		"reason": "x", "action": "reprovision", "filter": map[string]any{"status": "active"},
	})
	if w.Code != http.StatusBadRequest {
		t.Fatalf("expected 400 for destructive batch sub-action, got %d (%s)", w.Code, w.Body.String())
	}
}

// Each successful apply writes one admin.recovery.* event to Core.
func TestRecovery_ApplyWritesAuditEvent(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("t-aud", "Audit Co", map[string]interface{}{
		"subscription": map[string]interface{}{"tier": "indie", "status": "active"},
	}))
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	// reconcile-subscription is Guarded; apply (no dry_run) must write exactly one
	// admin.recovery.reconcile_subscription event.
	before := len(env.core.recoveryAudits())
	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-aud/reconcile-subscription", tok, map[string]any{"reason": "heal"})
	if w.Code != http.StatusOK {
		t.Fatalf("reconcile apply: expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	audits := env.core.recoveryAudits()
	if len(audits) != before+1 {
		t.Fatalf("expected exactly 1 new recovery audit, got %d", len(audits)-before)
	}
	last := audits[len(audits)-1]
	if last.EventType != "admin.recovery.reconcile_subscription" {
		t.Errorf("audit event_type = %s, want admin.recovery.reconcile_subscription", last.EventType)
	}
	if last.EntityID != "recovery:t-aud" {
		t.Errorf("audit entity_id = %s, want recovery:t-aud", last.EntityID)
	}
	if last.TenantID != usecases.RecoveryAuditTenant {
		t.Errorf("audit tenant = %s, want %s", last.TenantID, usecases.RecoveryAuditTenant)
	}
}

// The dry-run has to answer "is this key safe to revoke?", and a name cannot.
// Role, last use and expiry are what distinguish a key minted under old rules
// from one a live consumer depends on.
func TestRotateKeys_DryRunClassifiesEachKey(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("t-cls", "Classify Co", nil))
	used := time.Now().UTC().Add(-time.Hour)
	expires := time.Now().UTC().Add(24 * time.Hour)
	env.core.keysByTenant["t-cls"] = []clients.CoreAPIKeyInfo{
		// Legacy: off-role and never used. The safe first cohort.
		{ID: "k1", Name: "old", TenantID: "t-cls", Active: true, Role: "service_account"},
		// Current: canonical role, expiring, and in active use.
		{
			ID: "k2", Name: "live", TenantID: "t-cls", Active: true,
			Role: string(entities.RoleServiceAccount), ExpiresAt: &expires, LastUsed: &used,
		},
	}
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-cls/rotate-keys?dry_run=true", tok, map[string]any{"reason": "audit"})
	if w.Code != http.StatusOK {
		t.Fatalf("dry-run: expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	var res usecases.RecoveryResult
	if err := json.Unmarshal(w.Body.Bytes(), &res); err != nil {
		t.Fatalf("decode: %v", err)
	}

	if got := res.Would["legacy_unused"]; fmt.Sprint(got) != "1" {
		t.Errorf("legacy_unused = %v, want 1", got)
	}
	keys, ok := res.Would["keys"].([]any)
	if !ok || len(keys) != 2 {
		t.Fatalf("keys = %#v, want 2 entries", res.Would["keys"])
	}

	byName := map[string]map[string]any{}
	for _, k := range keys {
		m, ok := k.(map[string]any)
		if !ok {
			t.Fatalf("key entry is not an object: %#v", k)
		}
		byName[fmt.Sprint(m["name"])] = m
	}

	old := byName["old"]
	if old["legacy"] != true || old["never_used"] != true {
		t.Errorf("off-role never-used key not flagged: %#v", old)
	}
	if fmt.Sprint(old["role"]) != "service_account" {
		t.Errorf("role dropped on decode: %#v", old["role"])
	}

	live := byName["live"]
	if live["legacy"] != false || live["never_used"] != false {
		t.Errorf("canonical in-use key flagged as legacy: %#v", live)
	}
	if fmt.Sprint(live["last_used"]) == "" {
		t.Errorf("last_used dropped on decode: %#v", live)
	}
}

// rotate-keys end-to-end: dry-run → echo confirm_token → apply rotates + audits.
func TestRotateKeys_TokenGatedApply(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("t-rot", "Rotate Co", nil))
	env.core.keysByTenant["t-rot"] = []clients.CoreAPIKeyInfo{
		{ID: "k1", Name: "a", TenantID: "t-rot", Active: true},
		{ID: "k2", Name: "b", TenantID: "t-rot", Active: true},
	}
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	// 1. dry-run → confirm_token
	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-rot/rotate-keys?dry_run=true", tok, map[string]any{"reason": "rot"})
	var dr usecases.RecoveryResult
	_ = json.Unmarshal(w.Body.Bytes(), &dr) //nolint:errcheck // test decode
	if dr.ConfirmToken == "" {
		t.Fatalf("no confirm_token from dry-run")
	}
	// 2. apply WITHOUT the token → 400
	w = doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-rot/rotate-keys", tok, map[string]any{"reason": "rot"})
	if w.Code != http.StatusBadRequest {
		t.Fatalf("apply without token: expected 400, got %d", w.Code)
	}
	// 3. apply WITH the echoed token → 200, rotates + canonical role
	w = doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-rot/rotate-keys", tok, map[string]any{"reason": "rot", "confirm_token": dr.ConfirmToken})
	if w.Code != http.StatusOK {
		t.Fatalf("apply with token: expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	if len(env.core.created) != 1 {
		t.Errorf("expected 1 new key minted, got %d", len(env.core.created))
	}
	if env.core.created[0].Role != string(entities.RoleServiceAccount) {
		t.Errorf("new key role = %s, want serviceaccount", env.core.created[0].Role)
	}
	if len(env.core.revoked) != 2 {
		t.Errorf("expected 2 old keys revoked, got %d", len(env.core.revoked))
	}
}

// diagnose-identity flags a JWT/tenant divergence with a re-login fix (no mutation).
func TestDiagnoseIdentity_Divergence(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("data-tenant", "Data", nil))
	env.core.statsByTenant["data-tenant"] = 42
	r := env.router()
	// admin JWT claims tenant "system", asks about "data-tenant" → diverged.
	tok := adminToken(t, entities.RoleAdmin)

	w := doAdmin(t, r, http.MethodGet, "/api/v1/admin/recovery/data-tenant/diagnose-identity", tok, nil)
	if w.Code != http.StatusOK {
		t.Fatalf("expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	var resp struct {
		Diagnosis usecases.IdentityDiagnosis `json:"diagnosis"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &resp); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if !resp.Diagnosis.Diverged {
		t.Errorf("expected diverged=true (jwt tenant 'system' != 'data-tenant'); got %+v", resp.Diagnosis)
	}
	if !resp.Diagnosis.StoreHasData {
		t.Errorf("expected store_has_data=true for data-tenant")
	}
}

// ============================================================================
// reap-demo (gap #1 cleanup): cohort delete of Core is_demo tenants, guarded by
// the SAME recovery machinery (dry-run preview + echoed confirm_token + one
// admin.recovery.reap_demo audit event). Source of is_demo = Core's tenant list.
// ============================================================================

// Without an admin JWT the reaper is 403 (inherits AdminAuthMiddleware) — a raw
// probe can never delete tenants.
func TestReapDemo_RequiresAdmin(t *testing.T) {
	env := newFleetTestEnv(t, "")
	env.core.tenants = []clients.TenantResponse{{ID: "demo-1", Name: "Demo User", IsDemo: true}}
	r := env.router()

	// no token
	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/tenants/reap-demo?dry_run=true", "", nil)
	if w.Code != http.StatusUnauthorized && w.Code != http.StatusForbidden {
		t.Fatalf("no-token reap: expected 401/403, got %d (%s)", w.Code, w.Body.String())
	}
	// non-admin (developer) token
	devTok := adminToken(t, entities.RoleDeveloper)
	w = doAdmin(t, r, http.MethodPost, "/api/v1/admin/tenants/reap-demo?dry_run=true", devTok, nil)
	if w.Code != http.StatusForbidden {
		t.Fatalf("developer reap: expected 403, got %d (%s)", w.Code, w.Body.String())
	}
	if len(env.core.deletedTenants) != 0 {
		t.Fatalf("unauthorized reap must delete nothing, deleted %v", env.core.deletedTenants)
	}
}

// Dry-run lists exactly the is_demo tenants + count, returns a confirm_token, and
// mutates nothing. Non-demo tenants are excluded from the match.
func TestReapDemo_DryRunListsAndMutatesNothing(t *testing.T) {
	env := newFleetTestEnv(t, "")
	env.core.tenants = []clients.TenantResponse{
		{ID: "demo-1", Name: "Demo User", IsDemo: true},
		{ID: "demo-2", Name: "Demo User", IsDemo: true},
		{ID: "real-co", Name: "Real Co", IsDemo: false}, // must NOT match
	}
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/tenants/reap-demo?dry_run=true", tok, nil)
	if w.Code != http.StatusOK {
		t.Fatalf("dry-run: expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	var dr usecases.ReapDemoResult
	if err := json.Unmarshal(w.Body.Bytes(), &dr); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if dr.MatchedTotal != 2 {
		t.Errorf("matched_total = %d, want 2", dr.MatchedTotal)
	}
	if len(dr.Candidates) != 2 {
		t.Errorf("candidates = %d, want 2", len(dr.Candidates))
	}
	for _, cnd := range dr.Candidates {
		if cnd.ID == "real-co" {
			t.Errorf("non-demo tenant real-co must not be a reap candidate")
		}
	}
	if dr.ConfirmToken == "" {
		t.Errorf("dry-run must return a confirm_token")
	}
	if dr.Applied {
		t.Errorf("dry-run must not report applied")
	}
	if len(env.core.deletedTenants) != 0 {
		t.Errorf("dry-run must delete nothing, deleted %v", env.core.deletedTenants)
	}
}

// End-to-end: dry-run → apply without token (400) → apply with echoed token
// deletes exactly the demo tenants and writes exactly one admin.recovery.reap_demo
// audit event.
func TestReapDemo_TokenGatedApplyDeletesAndAudits(t *testing.T) {
	env := newFleetTestEnv(t, "")
	env.core.tenants = []clients.TenantResponse{
		{ID: "demo-1", Name: "Demo User", IsDemo: true},
		{ID: "demo-2", Name: "Demo User", IsDemo: true},
		{ID: "real-co", Name: "Real Co", IsDemo: false},
	}
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	// 1. dry-run → confirm_token, no audit-apply yet
	beforeAudits := len(env.core.recoveryAudits())
	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/tenants/reap-demo?dry_run=true", tok, map[string]any{"reason": "litter"})
	var dr usecases.ReapDemoResult
	_ = json.Unmarshal(w.Body.Bytes(), &dr) //nolint:errcheck // test decode
	if dr.ConfirmToken == "" {
		t.Fatalf("no confirm_token from dry-run")
	}

	// 2. apply WITHOUT the token → 400, still nothing deleted
	w = doAdmin(t, r, http.MethodPost, "/api/v1/admin/tenants/reap-demo", tok, map[string]any{"reason": "litter"})
	if w.Code != http.StatusBadRequest {
		t.Fatalf("apply without token: expected 400, got %d (%s)", w.Code, w.Body.String())
	}
	if len(env.core.deletedTenants) != 0 {
		t.Fatalf("apply without token must delete nothing, deleted %v", env.core.deletedTenants)
	}

	// 3. apply WITH the echoed token → 200, deletes exactly the 2 demo tenants
	auditsAfterDryRun := len(env.core.recoveryAudits())
	w = doAdmin(t, r, http.MethodPost, "/api/v1/admin/tenants/reap-demo", tok, map[string]any{"reason": "litter", "confirm_token": dr.ConfirmToken})
	if w.Code != http.StatusOK {
		t.Fatalf("apply with token: expected 200, got %d (%s)", w.Code, w.Body.String())
	}
	var ap usecases.ReapDemoResult
	if err := json.Unmarshal(w.Body.Bytes(), &ap); err != nil {
		t.Fatalf("unmarshal apply: %v", err)
	}
	if !ap.Applied {
		t.Errorf("apply must report applied=true")
	}
	if ap.Deleted != 2 {
		t.Errorf("deleted = %d, want 2", ap.Deleted)
	}
	if len(env.core.deletedTenants) != 2 {
		t.Fatalf("expected 2 deletes, got %v", env.core.deletedTenants)
	}
	for _, id := range env.core.deletedTenants {
		if id == "real-co" {
			t.Errorf("real-co (not is_demo) must never be deleted")
		}
	}

	// EXACTLY ONE admin.recovery.reap_demo apply audit event was written.
	applyAudits := 0
	for _, e := range env.core.recoveryAudits() {
		if e.EventType == "admin.recovery.reap_demo" {
			// apply audit has outcome "applied"
			if details, ok := e.Payload["outcome"].(string); ok && details == "applied" {
				applyAudits++
				if e.EntityID != "recovery:"+usecases.RecoveryAuditTenant {
					t.Errorf("reap_demo audit entity_id = %s, want recovery:%s", e.EntityID, usecases.RecoveryAuditTenant)
				}
			}
		}
	}
	if applyAudits != 1 {
		t.Fatalf("expected exactly 1 admin.recovery.reap_demo apply audit, got %d (before=%d afterDryRun=%d)", applyAudits, beforeAudits, auditsAfterDryRun)
	}
}

// itoaTest is a tiny local int→string for seeding (avoids strconv import churn).
func itoaTest(n int) string {
	if n == 0 {
		return "0"
	}
	var b [20]byte
	i := len(b)
	for n > 0 {
		i--
		b[i] = byte('0' + n%10)
		n /= 10
	}
	return string(b[i:])
}
