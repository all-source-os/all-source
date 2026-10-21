package usecases

import (
	"context"
	"errors"
	"fmt"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// changelogCoreFake enforces ExpectedVersion per entity, as Core does.
type changelogCoreFake struct {
	designPartnerCoreFake
}

func (f *changelogCoreFake) IngestEvent(ctx context.Context, req clients.IngestEventRequest) (*clients.IngestEventResponse, error) {
	if req.ExpectedVersion != nil {
		var version uint64
		for _, event := range f.events {
			if event.EntityID == req.EntityID {
				version++
			}
		}
		if version != *req.ExpectedVersion {
			f.ingests = append(f.ingests, req)
			return nil, clients.ErrVersionConflict
		}
	}
	return f.designPartnerCoreFake.IngestEvent(ctx, req)
}

// QueryEvents honors entity, prefix, order, offset and limit, and reports
// TotalCount before the limit, as Core does.
func (f *changelogCoreFake) QueryEvents(_ context.Context, req clients.QueryEventsRequest) (*clients.QueryEventsResponse, error) {
	if f.queryErr != nil {
		return nil, f.queryErr
	}
	matched := make([]clients.EventEntry, 0, len(f.events))
	for _, event := range f.events {
		if req.EntityID != "" && event.EntityID != req.EntityID {
			continue
		}
		if !strings.HasPrefix(event.EventType, req.EventTypePrefix) {
			continue
		}
		matched = append(matched, event)
	}
	total := len(matched)
	if req.Order == "desc" {
		slices.Reverse(matched)
	}
	start := min(req.Offset, len(matched))
	end := len(matched)
	if req.Limit > 0 {
		end = min(start+req.Limit, len(matched))
	}
	page := matched[start:end]
	return &clients.QueryEventsResponse{Events: page, Count: len(page), TotalCount: total}, nil
}

func validChangelogRequest() SubscribeChangelogRequest {
	return SubscribeChangelogRequest{
		Email: " Ada@Example.COM ", Consent: true, Placement: "home",
		CampaignSource: DesignPartnerCampaignSource{Source: "github", Medium: "community", Campaign: "allsource_2026-10"},
	}
}

func newChangelogUseCase(core *changelogCoreFake) *ChangelogUseCase {
	uc := NewChangelogUseCase(core, "test-signing-secret")
	uc.now = func() time.Time { return time.Date(2026, 10, 21, 12, 0, 0, 0, time.UTC) }
	return uc
}

func TestChangelogSubscribeWritesPrivateFirstVersionEvent(t *testing.T) {
	core := &changelogCoreFake{}
	uc := newChangelogUseCase(core)

	if err := uc.Subscribe(context.Background(), validChangelogRequest()); err != nil {
		t.Fatalf("Subscribe: %v", err)
	}
	if len(core.ingests) != 1 {
		t.Fatalf("ingests: got %d, want 1", len(core.ingests))
	}
	write := core.ingests[0]
	if write.TenantID != ChangelogTenant || write.EventType != ChangelogSubscribedEventType {
		t.Fatalf("wrong private stream: %#v", write)
	}
	if write.ExpectedVersion == nil || *write.ExpectedVersion != 0 {
		t.Fatalf("expected first-write version 0, got %#v", write.ExpectedVersion)
	}
	if write.Payload["email"] != "ada@example.com" || write.Payload["consent_version"] != ChangelogConsentVersion {
		t.Fatalf("payload not normalised: %#v", write.Payload)
	}
}

func TestChangelogSubscribeDedupesRepeatSignup(t *testing.T) {
	core := &changelogCoreFake{}
	uc := newChangelogUseCase(core)

	for range 2 {
		if err := uc.Subscribe(context.Background(), validChangelogRequest()); err != nil {
			t.Fatalf("Subscribe: %v", err)
		}
	}
	if len(core.events) != 1 {
		t.Fatalf("stored events: got %d, want 1", len(core.events))
	}
	subscribers, err := uc.List(context.Background())
	if err != nil || len(subscribers) != 1 {
		t.Fatalf("List: %v %#v", err, subscribers)
	}
}

func TestChangelogUnsubscribeThenResubscribe(t *testing.T) {
	core := &changelogCoreFake{}
	uc := newChangelogUseCase(core)
	ctx := context.Background()

	if err := uc.Subscribe(ctx, validChangelogRequest()); err != nil {
		t.Fatalf("Subscribe: %v", err)
	}
	id := changelogSubscriberID("ada@example.com")
	if err := uc.Unsubscribe(ctx, id); err != nil {
		t.Fatalf("Unsubscribe: %v", err)
	}
	if subscribers, err := uc.List(ctx); err != nil || len(subscribers) != 0 {
		t.Fatalf("unsubscribed address still listed: %#v", subscribers)
	}
	if err := uc.Unsubscribe(ctx, id); !errors.Is(err, ErrChangelogNotFound) {
		t.Fatalf("second unsubscribe: got %v, want not found", err)
	}
	if err := uc.Subscribe(ctx, validChangelogRequest()); err != nil {
		t.Fatalf("resubscribe: %v", err)
	}
	if subscribers, err := uc.List(ctx); err != nil || len(subscribers) != 1 {
		t.Fatalf("resubscribed address missing: %#v", subscribers)
	}
}

func TestChangelogSubscribeRejectsInvalidInput(t *testing.T) {
	cases := map[string]func(*SubscribeChangelogRequest){
		"bad email":     func(r *SubscribeChangelogRequest) { r.Email = "not-an-email" },
		"display name":  func(r *SubscribeChangelogRequest) { r.Email = "Ada <ada@example.com>" },
		"no consent":    func(r *SubscribeChangelogRequest) { r.Consent = false },
		"bad placement": func(r *SubscribeChangelogRequest) { r.Placement = "footer" },
	}
	for name, mutate := range cases {
		t.Run(name, func(t *testing.T) {
			core := &changelogCoreFake{}
			req := validChangelogRequest()
			mutate(&req)
			err := newChangelogUseCase(core).Subscribe(context.Background(), req)
			if !errors.Is(err, ErrChangelogInvalidInput) {
				t.Fatalf("got %v, want invalid input", err)
			}
			if len(core.ingests) != 0 {
				t.Fatal("invalid signup reached Core")
			}
		})
	}
}

func TestChangelogUnsubscribeWithTokenIsSelfServeAndRepeatable(t *testing.T) {
	core := &changelogCoreFake{}
	uc := newChangelogUseCase(core)
	ctx := context.Background()

	if err := uc.Subscribe(ctx, validChangelogRequest()); err != nil {
		t.Fatalf("Subscribe: %v", err)
	}
	subscribers, err := uc.List(ctx)
	if err != nil || len(subscribers) != 1 || subscribers[0].UnsubscribeToken == "" {
		t.Fatalf("List must carry an unsubscribe token: %v %#v", err, subscribers)
	}
	id, token := subscribers[0].ID, subscribers[0].UnsubscribeToken

	if err := uc.UnsubscribeWithToken(ctx, id, token[:len(token)-1]+"0"); !errors.Is(err, ErrChangelogInvalidInput) {
		t.Fatalf("tampered token: got %v, want invalid input", err)
	}
	if err := uc.UnsubscribeWithToken(ctx, changelogSubscriberID("eve@example.com"), token); !errors.Is(err, ErrChangelogInvalidInput) {
		t.Fatalf("token for another subscriber: got %v, want invalid input", err)
	}
	for range 2 {
		if err := uc.UnsubscribeWithToken(ctx, id, token); err != nil {
			t.Fatalf("UnsubscribeWithToken: %v", err)
		}
	}
	if subscribers, err := uc.List(ctx); err != nil || len(subscribers) != 0 {
		t.Fatalf("still listed after token unsubscribe: %#v", subscribers)
	}
}

func TestChangelogUnsubscribeWithTokenNeedsASigningSecret(t *testing.T) {
	uc := NewChangelogUseCase(&changelogCoreFake{}, "")
	if uc.UnsubscribeToken("0123456789abcdef0123456789abcdef") != "" {
		t.Fatal("token issued without a signing secret")
	}
	err := uc.UnsubscribeWithToken(context.Background(), "0123456789abcdef0123456789abcdef", "")
	if !errors.Is(err, ErrChangelogUnavailable) {
		t.Fatalf("got %v, want unavailable", err)
	}
}

func TestChangelogListReadsEveryPage(t *testing.T) {
	core := &changelogCoreFake{}
	uc := newChangelogUseCase(core)
	ctx := context.Background()

	const signups = changelogListPageSize + 5
	for i := range signups {
		req := validChangelogRequest()
		req.Email = fmt.Sprintf("reader%d@example.com", i)
		if err := uc.Subscribe(ctx, req); err != nil {
			t.Fatalf("Subscribe %d: %v", i, err)
		}
	}
	subscribers, err := uc.List(ctx)
	if err != nil || len(subscribers) != signups {
		t.Fatalf("List: got %d subscribers (%v), want %d", len(subscribers), err, signups)
	}
}

func TestChangelogResubscribeReadsTheNewestEventPastOnePage(t *testing.T) {
	core := &changelogCoreFake{}
	uc := newChangelogUseCase(core)
	ctx := context.Background()
	id := changelogSubscriberID("ada@example.com")

	for range 3 {
		if err := uc.Subscribe(ctx, validChangelogRequest()); err != nil {
			t.Fatalf("Subscribe: %v", err)
		}
		if err := uc.Unsubscribe(ctx, id); err != nil {
			t.Fatalf("Unsubscribe: %v", err)
		}
	}
	if err := uc.Subscribe(ctx, validChangelogRequest()); err != nil {
		t.Fatalf("final Subscribe: %v", err)
	}
	if subscribers, err := uc.List(ctx); err != nil || len(subscribers) != 1 {
		t.Fatalf("resubscribe after several cycles not listed: %#v", subscribers)
	}
}

func TestChangelogSubscribeReportsStorageFailure(t *testing.T) {
	core := &changelogCoreFake{designPartnerCoreFake{ingestErr: context.DeadlineExceeded}}
	err := newChangelogUseCase(core).Subscribe(context.Background(), validChangelogRequest())
	if !errors.Is(err, ErrChangelogUnavailable) {
		t.Fatalf("got %v, want unavailable", err)
	}
}
