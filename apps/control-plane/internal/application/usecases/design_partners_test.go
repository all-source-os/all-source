package usecases

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

type designPartnerCoreFake struct {
	clients.CoreClient
	events    []clients.EventEntry
	ingests   []clients.IngestEventRequest
	ingestErr error
	queryErr  error
}

func (f *designPartnerCoreFake) IngestEvent(_ context.Context, req clients.IngestEventRequest) (*clients.IngestEventResponse, error) {
	f.ingests = append(f.ingests, req)
	if f.ingestErr != nil {
		return nil, f.ingestErr
	}
	f.events = append(f.events, clients.EventEntry{
		ID: "event-test", EventType: req.EventType, EntityID: req.EntityID,
		Timestamp: "2026-08-27T12:00:00Z", Payload: req.Payload,
	})
	return &clients.IngestEventResponse{EventID: "event-test"}, nil
}

func (f *designPartnerCoreFake) QueryEvents(_ context.Context, req clients.QueryEventsRequest) (*clients.QueryEventsResponse, error) {
	if f.queryErr != nil {
		return nil, f.queryErr
	}
	filtered := make([]clients.EventEntry, 0, len(f.events))
	for _, event := range f.events {
		if req.EntityID != "" && event.EntityID != req.EntityID {
			continue
		}
		if req.EventTypePrefix != "" && len(event.EventType) >= len(req.EventTypePrefix) && event.EventType[:len(req.EventTypePrefix)] != req.EventTypePrefix {
			continue
		}
		filtered = append(filtered, event)
	}
	return &clients.QueryEventsResponse{Events: filtered, Count: len(filtered), TotalCount: len(filtered)}, nil
}

func validDesignPartnerRequest() SubmitDesignPartnerRequest {
	return SubmitDesignPartnerRequest{
		Name: "Ada Lovelace", Email: "ADA@EXAMPLE.COM", Project: "Recall Agent",
		AgentUseCase:  "A support agent that continues investigations across restarts.",
		MemoryProblem: "Current summaries lose provenance and silently overwrite earlier decisions.",
		Timeline:      "within_30_days", Consent: true,
		IdempotencyKey: "018f7e2d-ff2a-7db8-8cf4-000000000001",
		CampaignSource: DesignPartnerCampaignSource{Source: "dailydev", Medium: "community", Campaign: "design_partners_2026"},
	}
}

func TestDesignPartnerSubmitWritesPrivateIdempotentEvent(t *testing.T) {
	core := &designPartnerCoreFake{}
	uc := NewDesignPartnerUseCase(core)
	uc.now = func() time.Time { return time.Date(2026, 8, 27, 12, 0, 0, 0, time.UTC) }

	application, err := uc.Submit(context.Background(), validDesignPartnerRequest())
	if err != nil {
		t.Fatalf("Submit: %v", err)
	}
	if application.Email != "ada@example.com" {
		t.Fatalf("email not normalized: %q", application.Email)
	}
	if len(core.ingests) != 1 {
		t.Fatalf("ingests: got %d, want 1", len(core.ingests))
	}
	write := core.ingests[0]
	if write.TenantID != DesignPartnerTenant || write.EventType != DesignPartnerSubmittedEventType {
		t.Fatalf("wrong private stream: %#v", write)
	}
	if write.ExpectedVersion == nil || *write.ExpectedVersion != 0 {
		t.Fatalf("expected first-write version 0, got %#v", write.ExpectedVersion)
	}
	if write.Metadata["idempotency_key"] == "" {
		t.Fatal("idempotency metadata missing")
	}
	if _, leaked := write.Metadata["email"]; leaked {
		t.Fatal("PII must stay out of event metadata")
	}
}

func TestDesignPartnerSubmitTreatsVersionConflictAsDuplicate(t *testing.T) {
	core := &designPartnerCoreFake{ingestErr: clients.ErrVersionConflict}
	uc := NewDesignPartnerUseCase(core)
	first, err := uc.Submit(context.Background(), validDesignPartnerRequest())
	if err != nil {
		t.Fatalf("first duplicate response: %v", err)
	}
	second, err := uc.Submit(context.Background(), validDesignPartnerRequest())
	if err != nil {
		t.Fatalf("second duplicate response: %v", err)
	}
	if first.ID != second.ID {
		t.Fatalf("idempotency key produced different IDs: %q != %q", first.ID, second.ID)
	}
}

func TestDesignPartnerSubmissionValidation(t *testing.T) {
	tests := []struct {
		name   string
		mutate func(*SubmitDesignPartnerRequest)
	}{
		{"bad email", func(req *SubmitDesignPartnerRequest) { req.Email = "not-email" }},
		{"short use case", func(req *SubmitDesignPartnerRequest) { req.AgentUseCase = "short" }},
		{"unknown timeline", func(req *SubmitDesignPartnerRequest) { req.Timeline = "someday" }},
		{"no consent", func(req *SubmitDesignPartnerRequest) { req.Consent = false }},
		{"bad idempotency key", func(req *SubmitDesignPartnerRequest) { req.IdempotencyKey = "tiny" }},
		{"long source", func(req *SubmitDesignPartnerRequest) { req.CampaignSource.Source = string(make([]byte, 101)) }},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			req := validDesignPartnerRequest()
			test.mutate(&req)
			err := validateDesignPartnerSubmission(&req)
			if !errors.Is(err, ErrDesignPartnerInvalidInput) {
				t.Fatalf("got %v, want invalid input", err)
			}
		})
	}
}

func TestDesignPartnerListFoldsStatusHistory(t *testing.T) {
	core := &designPartnerCoreFake{}
	uc := NewDesignPartnerUseCase(core)
	uc.now = func() time.Time { return time.Date(2026, 8, 27, 12, 0, 0, 0, time.UTC) }
	application, err := uc.Submit(context.Background(), validDesignPartnerRequest())
	if err != nil {
		t.Fatalf("Submit: %v", err)
	}

	updated, err := uc.UpdateStatus(context.Background(), UpdateDesignPartnerStatusRequest{
		ApplicationID: application.ID, Status: "accepted", Actor: "admin-1", Note: "Strong production fit",
	})
	if err != nil {
		t.Fatalf("UpdateStatus: %v", err)
	}
	if updated.Status != "accepted" || len(updated.StatusHistory) != 2 {
		t.Fatalf("unexpected projection: %#v", updated)
	}
	wantRetention := time.Date(2027, 1, 24, 12, 0, 0, 0, time.UTC).Format(time.RFC3339Nano)
	if updated.RetentionUntil != wantRetention {
		t.Fatalf("retention: got %q, want %q", updated.RetentionUntil, wantRetention)
	}

	accepted, err := uc.List(context.Background(), "accepted")
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(accepted) != 1 || accepted[0].ID != application.ID {
		t.Fatalf("accepted projection: %#v", accepted)
	}
	statusWrite := core.ingests[1]
	if statusWrite.ExpectedVersion == nil || *statusWrite.ExpectedVersion != 1 {
		t.Fatalf("status expected version: %#v", statusWrite.ExpectedVersion)
	}
}

func TestDesignPartnerRejectRetentionIsNinetyDays(t *testing.T) {
	changedAt := time.Date(2026, 8, 27, 12, 0, 0, 0, time.UTC)
	got := designPartnerRetentionUntil("rejected", changedAt)
	want := changedAt.AddDate(0, 0, 90)
	if !got.Equal(want) {
		t.Fatalf("retention: got %s, want %s", got, want)
	}
}
