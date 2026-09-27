package usecases

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"testing"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// Optional operator validation uses a private local file, never a committed
// fixture. It performs no Core writes and does not print correspondence.
func TestPartnershipPrivateImportValidation(t *testing.T) {
	path := os.Getenv("PARTNERSHIP_IMPORT_PATH")
	if path == "" {
		t.Skip("No private import requested")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal("Could not read private import")
	}
	var packet struct {
		Records []PartnershipRecord `json:"records"`
	}
	if json.Unmarshal(data, &packet) != nil || len(packet.Records) == 0 {
		t.Fatal("Invalid private import")
	}
	seen := map[string]bool{}
	for i := range packet.Records {
		if err := validatePartnership(&packet.Records[i]); err != nil {
			t.Fatalf("Record %d validation: %v", i+1, err)
		}
		if seen[packet.Records[i].ID] {
			t.Fatalf("Duplicate identity at record %d", i+1)
		}
		seen[packet.Records[i].ID] = true
	}
	t.Logf("Validated %d private records; no writes or sends", len(packet.Records))
}

type partnershipFake struct {
	clients.CoreClient
	events              []clients.EventEntry
	writes              []clients.IngestEventRequest
	queries             []clients.QueryEventsRequest
	failRead, failWrite bool
}

func (f *partnershipFake) QueryEvents(_ context.Context, q clients.QueryEventsRequest) (*clients.QueryEventsResponse, error) {
	f.queries = append(f.queries, q)
	if f.failRead {
		return nil, errors.New("private upstream failure")
	}
	var matches []clients.EventEntry
	for _, e := range f.events {
		if q.EntityID == "" || q.EntityID == e.EntityID {
			matches = append(matches, e)
		}
	}
	total := len(matches)
	start := min(q.Offset, total)
	end := min(start+q.Limit, total)
	return &clients.QueryEventsResponse{Events: matches[start:end], TotalCount: total}, nil
}
func (f *partnershipFake) IngestEvent(_ context.Context, r clients.IngestEventRequest) (*clients.IngestEventResponse, error) {
	if f.failWrite {
		return nil, errors.New("write failed")
	}
	var n uint64
	for _, e := range f.events {
		if e.EntityID == r.EntityID {
			n++
		}
	}
	if r.ExpectedVersion == nil || n != *r.ExpectedVersion {
		return nil, clients.ErrVersionConflict
	}
	f.writes = append(f.writes, r)
	f.events = append(f.events, clients.EventEntry{ID: fmt.Sprint(len(f.events)), EntityID: r.EntityID, EventType: r.EventType, Payload: r.Payload})
	return &clients.IngestEventResponse{ID: "saved"}, nil
}
func partnershipFixture() PartnershipRecord {
	return PartnershipRecord{Organization: "Example Ventures", Kind: "vc", Website: "https://www.example.test", Status: "research", Sources: []PartnershipSource{}, Messages: []PartnershipMessage{}}
}
func TestPartnershipPersistenceAndRevisionConflict(t *testing.T) {
	f := &partnershipFake{}
	u := NewPartnershipsUseCase(f)
	zero := uint64(0)
	first, err := u.Save(t.Context(), SavePartnershipRequest{Record: partnershipFixture(), ExpectedRevision: &zero}, "operator")
	if err != nil {
		t.Fatal(err)
	}
	if first.Record.ID != "example.test" || first.Revision != 1 {
		t.Fatalf("bad canonical identity: %+v", first)
	}
	if f.writes[0].TenantID != partnershipTenant || f.writes[0].Metadata != nil {
		t.Fatal("private data must be isolated in payload")
	}
	if _, err = u.Save(t.Context(), SavePartnershipRequest{Record: partnershipFixture(), ExpectedRevision: &zero}, "operator"); !errors.Is(err, ErrPartnershipConflict) {
		t.Fatalf("duplicate create: %v", err)
	}
	changed := first.Record
	changed.NextAction = "Review route"
	second, err := u.Save(t.Context(), SavePartnershipRequest{Record: changed, ExpectedRevision: &first.Revision}, "operator")
	if err != nil || second.Revision != 2 {
		t.Fatalf("update: %v", err)
	}
	rebuilt, err := NewPartnershipsUseCase(f).List(t.Context())
	if err != nil || len(rebuilt) != 1 || rebuilt[0].Record.NextAction != "Review route" {
		t.Fatalf("rebuild: %+v %v", rebuilt, err)
	}
	for _, q := range f.queries {
		if q.TenantID != partnershipTenant || q.EventType != partnershipEvent {
			t.Fatal("unscoped query")
		}
	}
}
func TestPartnershipValidationAndWriteFailure(t *testing.T) {
	zero := uint64(0)
	u := NewPartnershipsUseCase(&partnershipFake{})
	for _, change := range []func(*PartnershipRecord){
		func(r *PartnershipRecord) { r.Website = "javascript:alert(1)" },
		func(r *PartnershipRecord) { r.ID = "other.test" },
		func(r *PartnershipRecord) {
			r.Sources = []PartnershipSource{{URL: "file:///private", Title: "Local", Evidence: "No", CheckedAt: "2026-09-26T00:00:00Z"}}
		},
		func(r *PartnershipRecord) {
			r.Score = &PartnershipScore{Model: "Jev", RunAt: "2026-09-26T00:00:00Z", Rationale: "Snapshot", Fit: 4}
		},
		func(r *PartnershipRecord) {
			r.Messages = []PartnershipMessage{{ID: "1", Channel: "email", Direction: "outbound", Outcome: "sent", Body: "Hello", Destination: "hello@example.test", OccurredAt: "2026-09-26T00:00:00Z"}}
		},
	} {
		r := partnershipFixture()
		change(&r)
		if _, err := u.Save(t.Context(), SavePartnershipRequest{r, &zero}, "operator"); !errors.Is(err, ErrPartnershipInvalid) {
			t.Fatalf("accepted invalid record: %v", err)
		}
	}
	if _, err := u.Save(t.Context(), SavePartnershipRequest{Record: partnershipFixture()}, "operator"); !errors.Is(err, ErrPartnershipInvalid) {
		t.Fatal("missing revision accepted")
	}
	f := &partnershipFake{failWrite: true}
	if _, err := NewPartnershipsUseCase(f).Save(t.Context(), SavePartnershipRequest{partnershipFixture(), &zero}, "operator"); !errors.Is(err, ErrPartnershipUnavailable) {
		t.Fatalf("write failure: %v", err)
	}
}
func TestPartnershipProtectsSentHistoryAndSuppression(t *testing.T) {
	u := NewPartnershipsUseCase(&partnershipFake{})
	r := partnershipFixture()
	r.Status = "do_not_contact"
	r.Messages = []PartnershipMessage{{ID: "message-1", Channel: "email", Direction: "outbound", Outcome: "sent", Destination: "hello@example.test", Body: "Historical message", OccurredAt: "2026-09-26T12:00:00Z", Verification: "Provider SENT id example"}}
	zero := uint64(0)
	saved, err := u.Save(t.Context(), SavePartnershipRequest{r, &zero}, "operator")
	if err != nil {
		t.Fatal(err)
	}
	r = saved.Record
	r.Messages = []PartnershipMessage{}
	if _, err = u.Save(t.Context(), SavePartnershipRequest{r, &saved.Revision}, "operator"); !errors.Is(err, ErrPartnershipInvalid) {
		t.Fatal("deleted immutable send")
	}
	r = saved.Record
	r.Status = "ready"
	if _, err = u.Save(t.Context(), SavePartnershipRequest{r, &saved.Revision}, "operator"); !errors.Is(err, ErrPartnershipInvalid) {
		t.Fatal("removed suppression")
	}
}
func TestPartnershipReadsBeyondFirstPageAndFailsClosed(t *testing.T) {
	f := &partnershipFake{}
	for i := 0; i < 501; i++ {
		f.events = append(f.events, clients.EventEntry{EventType: partnershipEvent, EntityID: fmt.Sprintf("partnership:%d.example.test", i), Payload: map[string]any{"revision": 1, "record": map[string]any{"id": fmt.Sprintf("%d.example.test", i), "organization": fmt.Sprint(i)}}})
	}
	u := NewPartnershipsUseCase(f)
	list, err := u.List(t.Context())
	if err != nil || len(list) != 501 || len(f.queries) != 2 {
		t.Fatalf("pagination: %d %v", len(list), err)
	}
	f.failRead = true
	if _, err = u.List(t.Context()); !errors.Is(err, ErrPartnershipUnavailable) {
		t.Fatal("read failure masked")
	}
	f.failRead = false
	f.events[0].Payload = map[string]any{"record": "broken"}
	if _, err = u.List(t.Context()); !errors.Is(err, ErrPartnershipUnavailable) {
		t.Fatal("corrupt record hidden")
	}
}
