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
	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

type designPartnerHandlerCoreFake struct {
	clients.CoreClient
	write clients.IngestEventRequest
	err   error
}

func (f *designPartnerHandlerCoreFake) IngestEvent(_ context.Context, req clients.IngestEventRequest) (*clients.IngestEventResponse, error) {
	f.write = req
	if f.err != nil {
		return nil, f.err
	}
	return &clients.IngestEventResponse{EventID: "event-1"}, nil
}

func designPartnerHandlerRequest(t *testing.T, cp *ControlPlane, body map[string]any) *httptest.ResponseRecorder {
	t.Helper()
	encoded, err := json.Marshal(body)
	if err != nil {
		t.Fatalf("marshal body: %v", err)
	}
	recorder := httptest.NewRecorder()
	ctx, _ := gin.CreateTestContext(recorder)
	ctx.Request = httptest.NewRequestWithContext(t.Context(), http.MethodPost, "/api/v1/design-partners/applications", bytes.NewReader(encoded))
	ctx.Request.Header.Set("content-type", "application/json")
	cp.DesignPartnerApplyHandler(ctx)
	return recorder
}

func validDesignPartnerHandlerBody() map[string]any {
	return map[string]any{
		"name": "Ada Lovelace", "email": "ada@example.com", "project": "Recall Agent",
		"agent_use_case": "A support agent that continues investigations across process restarts.",
		"memory_problem": "Current summaries lose provenance and silently overwrite earlier decisions.",
		"timeline":       "within_30_days", "consent": true,
		"idempotency_key": "018f7e2d-ff2a-7db8-8cf4-000000000001",
	}
}

func TestDesignPartnerApplyHandlerReturnsOpaqueSuccess(t *testing.T) {
	gin.SetMode(gin.TestMode)
	core := &designPartnerHandlerCoreFake{}
	cp := &ControlPlane{container: &controlinternal.Container{DesignPartnerUC: usecases.NewDesignPartnerUseCase(core)}}

	recorder := designPartnerHandlerRequest(t, cp, validDesignPartnerHandlerBody())
	if recorder.Code != http.StatusCreated {
		t.Fatalf("status: got %d body=%s", recorder.Code, recorder.Body.String())
	}
	var response map[string]any
	if err := json.Unmarshal(recorder.Body.Bytes(), &response); err != nil {
		t.Fatalf("decode response: %v", err)
	}
	if response["application_id"] == "" || response["status"] != "new" {
		t.Fatalf("unexpected response: %#v", response)
	}
	if _, leaked := response["email"]; leaked {
		t.Fatal("handler echoed applicant PII")
	}
}

func TestDesignPartnerApplyHandlerRequiresCaptchaWhenConfigured(t *testing.T) {
	cp := &ControlPlane{
		container: &controlinternal.Container{DesignPartnerUC: usecases.NewDesignPartnerUseCase(&designPartnerHandlerCoreFake{})},
		turnstile: &TurnstileVerifier{secretKey: "configured"},
	}

	recorder := designPartnerHandlerRequest(t, cp, validDesignPartnerHandlerBody())
	if recorder.Code != http.StatusForbidden {
		t.Fatalf("status: got %d body=%s", recorder.Code, recorder.Body.String())
	}
}

func TestDesignPartnerApplyHandlerHidesStorageFailure(t *testing.T) {
	core := &designPartnerHandlerCoreFake{err: context.DeadlineExceeded}
	cp := &ControlPlane{container: &controlinternal.Container{DesignPartnerUC: usecases.NewDesignPartnerUseCase(core)}}

	recorder := designPartnerHandlerRequest(t, cp, validDesignPartnerHandlerBody())
	if recorder.Code != http.StatusServiceUnavailable {
		t.Fatalf("status: got %d body=%s", recorder.Code, recorder.Body.String())
	}
	if bytes.Contains(recorder.Body.Bytes(), []byte("deadline")) {
		t.Fatalf("private upstream error leaked: %s", recorder.Body.String())
	}
}
