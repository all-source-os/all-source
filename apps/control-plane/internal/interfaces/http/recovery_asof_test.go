package http

import (
	"net/http"
	"testing"

	"github.com/allsource/control-plane/internal/domain/entities"
)

// A dropped as_of would widen the restore to full history, so it is a 400.
func TestRestore_RejectsUnparseableAsOf(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("t-rs", "Restore", map[string]interface{}{
		"subscription": map[string]interface{}{"tier": "indie", "status": "active"},
	}))
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	body := map[string]any{"reason": "rebuild", "snapshot_id": "s-1", "as_of": "last tuesday"}
	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-rs/restore?dry_run=true", tok, body)
	if w.Code != http.StatusBadRequest {
		t.Fatalf("dry-run with bad as_of: expected 400, got %d (%s)", w.Code, w.Body.String())
	}

	body["confirm_tenant_id"] = "t-rs"
	w = doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-rs/restore", tok, body)
	if w.Code != http.StatusBadRequest {
		t.Fatalf("apply with bad as_of: expected 400, got %d (%s)", w.Code, w.Body.String())
	}
	if n := len(env.core.recoveryAudits()); n != 0 {
		t.Errorf("rejected restore must not write a recovery audit, got %d", n)
	}
}

func TestRestore_AcceptsNanosecondAsOf(t *testing.T) {
	env := newFleetTestEnv(t, "")
	seed(t, env.repo, mustActiveTenant("t-rn", "Restore", map[string]interface{}{
		"subscription": map[string]interface{}{"tier": "indie", "status": "active"},
	}))
	r := env.router()
	tok := adminToken(t, entities.RoleAdmin)

	w := doAdmin(t, r, http.MethodPost, "/api/v1/admin/recovery/t-rn/restore?dry_run=true", tok, map[string]any{
		"reason": "rebuild", "snapshot_id": "s-1", "as_of": "2026-01-02T03:04:05.987654321Z",
	})
	if w.Code != http.StatusOK {
		t.Fatalf("dry-run with nanosecond as_of: expected 200, got %d (%s)", w.Code, w.Body.String())
	}
}
