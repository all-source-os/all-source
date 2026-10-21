package usecases

import (
	"context"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"net/mail"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// Changelog-list vocabulary. Every value here is persisted inside Core events,
// so changing one rewrites the meaning of history rather than renaming a symbol.
const (
	// ChangelogTenant isolates subscriber addresses from customer and public streams.
	ChangelogTenant = "admin-changelog-subscribers"

	ChangelogSubscribedEventType   = "changelog.subscribed"
	ChangelogUnsubscribedEventType = "changelog.unsubscribed"
	ChangelogConsentVersion        = "2026-10-21"
)

// Sentinel errors matched with errors.Is in changelogError; anything not listed
// there falls through to a 503.
var (
	ErrChangelogInvalidInput = errors.New("changelog: invalid input")
	ErrChangelogNotFound     = errors.New("changelog: subscriber not found")
	ErrChangelogUnavailable  = errors.New("changelog: storage unavailable")

	subscriberIDPattern = regexp.MustCompile(`^[a-f0-9]{32}$`)
)

const changelogListPageSize = 1000

// ChangelogUseCase stores changelog-list consent as append-only Core events.
// The entity ID is derived from the normalised address, so a repeat signup
// folds onto the same entity instead of creating a duplicate.
type ChangelogUseCase struct {
	core           designPartnerCore
	now            func() time.Time
	unsubscribeKey []byte
}

// NewChangelogUseCase binds the use case to a Core client and the wall clock.
// The signing secret keys unsubscribe tokens; an empty secret disables
// token-based unsubscribe.
func NewChangelogUseCase(core clients.CoreClient, signingSecret string) *ChangelogUseCase {
	uc := &ChangelogUseCase{core: core, now: time.Now}
	if signingSecret != "" {
		key := sha256.Sum256([]byte("changelog-unsubscribe-key|" + signingSecret))
		uc.unsubscribeKey = key[:]
	}
	return uc
}

// SubscribeChangelogRequest is the public signup payload.
type SubscribeChangelogRequest struct {
	Email             string                      `json:"email"`
	Consent           bool                        `json:"consent"`
	Placement         string                      `json:"placement"`
	CampaignSource    DesignPartnerCampaignSource `json:"campaign_source"`
	TurnstileResponse string                      `json:"cf_turnstile_response,omitempty"`
}

// ChangelogSubscriber is the projection folded from one subscriber's events.
// UnsubscribeToken goes into the one-click unsubscribe link of every email.
type ChangelogSubscriber struct {
	ID               string                      `json:"id"`
	Email            string                      `json:"email"`
	Placement        string                      `json:"placement"`
	SubscribedAt     string                      `json:"subscribed_at"`
	ConsentVersion   string                      `json:"consent_version"`
	CampaignSource   DesignPartnerCampaignSource `json:"campaign_source"`
	UnsubscribeToken string                      `json:"unsubscribe_token,omitempty"`
}

// Subscribe records consent. It returns nil for both a new and an existing
// subscriber so the public response never reveals whether an address is listed.
func (uc *ChangelogUseCase) Subscribe(ctx context.Context, req SubscribeChangelogRequest) error {
	if uc == nil || uc.core == nil {
		return ErrChangelogUnavailable
	}
	if err := validateChangelogSubscription(&req); err != nil {
		return err
	}

	subscriberID := changelogSubscriberID(req.Email)
	payload := map[string]any{
		"subscriber_id":   subscriberID,
		"email":           req.Email,
		"placement":       req.Placement,
		"subscribed_at":   uc.now().UTC().Format(time.RFC3339Nano),
		"consent_version": ChangelogConsentVersion,
		"campaign_source": campaignSourcePayload(req.CampaignSource),
	}

	var firstVersion uint64
	err := uc.write(ctx, ChangelogSubscribedEventType, subscriberID, payload, firstVersion)
	if err == nil {
		return nil
	}
	if !errors.Is(err, clients.ErrVersionConflict) {
		return fmt.Errorf("%w: write subscription", ErrChangelogUnavailable)
	}

	// The entity exists. Only an address that unsubscribed earlier needs a new
	// event; an active subscriber is already on the list.
	latest, total, err := uc.latestEvent(ctx, subscriberID)
	if err != nil {
		return err
	}
	if latest == nil || latest.EventType != ChangelogUnsubscribedEventType {
		return nil
	}
	if err := uc.write(ctx, ChangelogSubscribedEventType, subscriberID, payload, total); err != nil &&
		!errors.Is(err, clients.ErrVersionConflict) {
		return fmt.Errorf("%w: write resubscription", ErrChangelogUnavailable)
	}
	return nil
}

// List folds the tenant and returns active subscribers, newest first.
func (uc *ChangelogUseCase) List(ctx context.Context) ([]ChangelogSubscriber, error) {
	if uc == nil || uc.core == nil {
		return nil, ErrChangelogUnavailable
	}
	var events []clients.EventEntry
	for offset := 0; ; offset += changelogListPageSize {
		resp, err := uc.core.QueryEvents(ctx, clients.QueryEventsRequest{
			TenantID:        ChangelogTenant,
			EventTypePrefix: "changelog.",
			Limit:           changelogListPageSize,
			Offset:          offset,
			Order:           "asc",
		})
		if err != nil {
			return nil, fmt.Errorf("%w: read subscribers", ErrChangelogUnavailable)
		}
		page := eventsFromResponse(resp)
		events = append(events, page...)
		if len(page) < changelogListPageSize {
			break
		}
	}
	subscribers := projectChangelogSubscribers(events)
	for i := range subscribers {
		subscribers[i].UnsubscribeToken = uc.UnsubscribeToken(subscribers[i].ID)
	}
	return subscribers, nil
}

// UnsubscribeToken is the HMAC that authorizes a one-click unsubscribe for one
// subscriber. It is empty when no signing secret is configured.
func (uc *ChangelogUseCase) UnsubscribeToken(subscriberID string) string {
	if uc == nil || len(uc.unsubscribeKey) == 0 {
		return ""
	}
	mac := hmac.New(sha256.New, uc.unsubscribeKey)
	mac.Write([]byte(subscriberID))
	return hex.EncodeToString(mac.Sum(nil))
}

// UnsubscribeWithToken is the self-serve path behind the link in every email.
// A valid token for an address that is already off the list succeeds, so the
// link can be followed twice.
func (uc *ChangelogUseCase) UnsubscribeWithToken(ctx context.Context, subscriberID, token string) error {
	if uc == nil || len(uc.unsubscribeKey) == 0 {
		return ErrChangelogUnavailable
	}
	subscriberID = strings.TrimSpace(subscriberID)
	expected := uc.UnsubscribeToken(subscriberID)
	if !subscriberIDPattern.MatchString(subscriberID) ||
		!hmac.Equal([]byte(expected), []byte(strings.TrimSpace(token))) {
		return fmt.Errorf("%w: invalid unsubscribe link", ErrChangelogInvalidInput)
	}
	if err := uc.Unsubscribe(ctx, subscriberID); err != nil && !errors.Is(err, ErrChangelogNotFound) {
		return err
	}
	return nil
}

// Unsubscribe appends an unsubscribe event for an active subscriber. The
// projection drops the address once it is folded.
func (uc *ChangelogUseCase) Unsubscribe(ctx context.Context, subscriberID string) error {
	if uc == nil || uc.core == nil {
		return ErrChangelogUnavailable
	}
	subscriberID = strings.TrimSpace(subscriberID)
	if !subscriberIDPattern.MatchString(subscriberID) {
		return fmt.Errorf("%w: invalid subscriber id", ErrChangelogInvalidInput)
	}
	latest, total, err := uc.latestEvent(ctx, subscriberID)
	if err != nil {
		return err
	}
	if latest == nil || latest.EventType != ChangelogSubscribedEventType {
		return ErrChangelogNotFound
	}
	payload := map[string]any{
		"subscriber_id":   subscriberID,
		"unsubscribed_at": uc.now().UTC().Format(time.RFC3339Nano),
	}
	if err := uc.write(ctx, ChangelogUnsubscribedEventType, subscriberID, payload, total); err != nil {
		return fmt.Errorf("%w: write unsubscribe", ErrChangelogUnavailable)
	}
	return nil
}

func (uc *ChangelogUseCase) write(ctx context.Context, eventType, subscriberID string, payload map[string]any, version uint64) error {
	_, err := uc.core.IngestEvent(ctx, clients.IngestEventRequest{
		EventType:       eventType,
		EntityID:        changelogEntityID(subscriberID),
		TenantID:        ChangelogTenant,
		Payload:         payload,
		ExpectedVersion: &version,
	})
	return err
}

// latestEvent returns the subscriber's newest event and the entity's event
// count, which is the expected version for the next write.
func (uc *ChangelogUseCase) latestEvent(ctx context.Context, subscriberID string) (*clients.EventEntry, uint64, error) {
	resp, err := uc.core.QueryEvents(ctx, clients.QueryEventsRequest{
		TenantID: ChangelogTenant,
		EntityID: changelogEntityID(subscriberID),
		Limit:    1,
		Order:    "desc",
	})
	if err != nil {
		return nil, 0, fmt.Errorf("%w: read subscriber", ErrChangelogUnavailable)
	}
	events := eventsFromResponse(resp)
	if len(events) == 0 {
		return nil, 0, nil
	}
	total := uint64(len(events))
	// TotalCount comes off the wire; the >= 0 guards the unsigned conversion.
	if resp != nil && resp.TotalCount >= 0 && resp.TotalCount > len(events) {
		total = uint64(resp.TotalCount)
	}
	return &events[0], total, nil
}

func validateChangelogSubscription(req *SubscribeChangelogRequest) error {
	req.Email = strings.ToLower(strings.TrimSpace(req.Email))
	req.Placement = strings.TrimSpace(strings.ToLower(req.Placement))
	req.CampaignSource = normalizeCampaignSource(req.CampaignSource)

	address, err := mail.ParseAddress(req.Email)
	if err != nil || address.Address != req.Email || len(req.Email) > 254 {
		return fmt.Errorf("%w: valid email required", ErrChangelogInvalidInput)
	}
	if !req.Consent {
		return fmt.Errorf("%w: consent required", ErrChangelogInvalidInput)
	}
	switch req.Placement {
	case "home", "prime":
	default:
		return fmt.Errorf("%w: unsupported placement", ErrChangelogInvalidInput)
	}
	if campaignSourceTooLong(req.CampaignSource) {
		return fmt.Errorf("%w: campaign source value too long", ErrChangelogInvalidInput)
	}
	return nil
}

func changelogSubscriberID(email string) string {
	hash := sha256.Sum256([]byte("changelog|" + email))
	return hex.EncodeToString(hash[:16])
}

func changelogEntityID(subscriberID string) string {
	return "changelog-subscriber:" + subscriberID
}

func projectChangelogSubscribers(events []clients.EventEntry) []ChangelogSubscriber {
	byID := make(map[string]*ChangelogSubscriber)
	for _, event := range events {
		subscriberID := payloadString(event.Payload, "subscriber_id")
		if subscriberID == "" {
			continue
		}
		switch event.EventType {
		case ChangelogSubscribedEventType:
			subscribedAt := payloadString(event.Payload, "subscribed_at")
			if subscribedAt == "" {
				subscribedAt = event.Timestamp
			}
			byID[subscriberID] = &ChangelogSubscriber{
				ID: subscriberID, Email: payloadString(event.Payload, "email"),
				Placement: payloadString(event.Payload, "placement"), SubscribedAt: subscribedAt,
				ConsentVersion: payloadString(event.Payload, "consent_version"),
				CampaignSource: campaignSourceFromPayload(event.Payload["campaign_source"]),
			}
		case ChangelogUnsubscribedEventType:
			delete(byID, subscriberID)
		}
	}

	result := make([]ChangelogSubscriber, 0, len(byID))
	for _, subscriber := range byID {
		result = append(result, *subscriber)
	}
	sort.Slice(result, func(i, j int) bool { return result[i].SubscribedAt > result[j].SubscribedAt })
	return result
}
