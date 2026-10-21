package main

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"

	controlinternal "github.com/allsource/control-plane/internal"
	"github.com/allsource/control-plane/internal/application/usecases"
)

func changelogHandlerRequest(t *testing.T, cp *ControlPlane, body map[string]any) *httptest.ResponseRecorder {
	t.Helper()
	encoded, err := json.Marshal(body)
	if err != nil {
		t.Fatalf("marshal body: %v", err)
	}
	recorder := httptest.NewRecorder()
	ctx, _ := gin.CreateTestContext(recorder)
	ctx.Request = httptest.NewRequestWithContext(t.Context(), http.MethodPost, "/api/v1/changelog/subscriptions", bytes.NewReader(encoded))
	ctx.Request.Header.Set("content-type", "application/json")
	cp.ChangelogSubscribeHandler(ctx)
	return recorder
}

func validChangelogHandlerBody() map[string]any {
	return map[string]any{"email": "ada@example.com", "consent": true, "placement": "prime"}
}

func TestChangelogSubscribeHandlerDoesNotEchoAddress(t *testing.T) {
	gin.SetMode(gin.TestMode)
	core := &designPartnerHandlerCoreFake{}
	cp := &ControlPlane{container: &controlinternal.Container{ChangelogUC: usecases.NewChangelogUseCase(core, "test-signing-secret")}}

	recorder := changelogHandlerRequest(t, cp, validChangelogHandlerBody())
	if recorder.Code != http.StatusOK {
		t.Fatalf("status: got %d body=%s", recorder.Code, recorder.Body.String())
	}
	if bytes.Contains(recorder.Body.Bytes(), []byte("ada@example.com")) {
		t.Fatalf("handler echoed the address: %s", recorder.Body.String())
	}
	if core.write.TenantID != usecases.ChangelogTenant {
		t.Fatalf("wrote outside the private tenant: %#v", core.write)
	}
}

func TestChangelogSubscribeHandlerRejectsMissingConsent(t *testing.T) {
	core := &designPartnerHandlerCoreFake{}
	cp := &ControlPlane{container: &controlinternal.Container{ChangelogUC: usecases.NewChangelogUseCase(core, "test-signing-secret")}}
	body := validChangelogHandlerBody()
	body["consent"] = false

	recorder := changelogHandlerRequest(t, cp, body)
	if recorder.Code != http.StatusBadRequest {
		t.Fatalf("status: got %d body=%s", recorder.Code, recorder.Body.String())
	}
	if core.write.EventType != "" {
		t.Fatal("rejected signup reached Core")
	}
}

func TestChangelogSubscribeHandlerRequiresCaptchaWhenConfigured(t *testing.T) {
	cp := &ControlPlane{
		container: &controlinternal.Container{ChangelogUC: usecases.NewChangelogUseCase(&designPartnerHandlerCoreFake{}, "test-signing-secret")},
		turnstile: &TurnstileVerifier{secretKey: "configured"},
	}

	recorder := changelogHandlerRequest(t, cp, validChangelogHandlerBody())
	if recorder.Code != http.StatusForbidden {
		t.Fatalf("status: got %d body=%s", recorder.Code, recorder.Body.String())
	}
}

func TestChangelogSubscribeHandlerHidesStorageFailure(t *testing.T) {
	core := &designPartnerHandlerCoreFake{err: context.DeadlineExceeded}
	cp := &ControlPlane{container: &controlinternal.Container{ChangelogUC: usecases.NewChangelogUseCase(core, "test-signing-secret")}}

	recorder := changelogHandlerRequest(t, cp, validChangelogHandlerBody())
	if recorder.Code != http.StatusServiceUnavailable {
		t.Fatalf("status: got %d body=%s", recorder.Code, recorder.Body.String())
	}
	if bytes.Contains(recorder.Body.Bytes(), []byte("deadline")) {
		t.Fatalf("private upstream error leaked: %s", recorder.Body.String())
	}
}
