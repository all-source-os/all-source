package usecases

import (
	"context"
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

// Design-partner vocabulary. Every value here is persisted inside Core events,
// so changing one rewrites the meaning of history rather than renaming a symbol.
const (
	// DesignPartnerTenant isolates applicant PII from customer and public streams.
	DesignPartnerTenant = "admin-design-partners"

	DesignPartnerSubmittedEventType = "design_partner.application_submitted"
	DesignPartnerStatusEventType    = "design_partner.status_changed"
	DesignPartnerConsentVersion     = "2026-08-27"

	DesignPartnerStatusNew        = "new"
	DesignPartnerStatusReviewing  = "reviewing"
	DesignPartnerStatusAccepted   = "accepted"
	DesignPartnerStatusWaitlisted = "waitlisted"
	DesignPartnerStatusRejected   = "rejected"
)

// Sentinel errors matched with errors.Is in designPartnerError; anything not
// listed there falls through to a 503, so a new one needs a case added too.
var (
	ErrDesignPartnerInvalidInput = errors.New("design partner: invalid input")
	ErrDesignPartnerNotFound     = errors.New("design partner: application not found")
	ErrDesignPartnerUnavailable  = errors.New("design partner: storage unavailable")

	idempotencyKeyPattern = regexp.MustCompile(`^[A-Za-z0-9_-]{16,100}$`)
	applicationIDPattern  = regexp.MustCompile(`^[a-f0-9]{32}$`)
)

type designPartnerCore interface {
	IngestEvent(context.Context, clients.IngestEventRequest) (*clients.IngestEventResponse, error)
	QueryEvents(context.Context, clients.QueryEventsRequest) (*clients.QueryEventsResponse, error)
}

// DesignPartnerUseCase stores applications and review decisions as append-only
// Core events. It is the sole owner of application validation and projection.
type DesignPartnerUseCase struct {
	core designPartnerCore
	now  func() time.Time
}

// NewDesignPartnerUseCase binds the use case to a Core client and the wall clock.
func NewDesignPartnerUseCase(core clients.CoreClient) *DesignPartnerUseCase {
	return &DesignPartnerUseCase{core: core, now: time.Now}
}

// DesignPartnerCampaignSource carries the UTM attribution captured at submission.
type DesignPartnerCampaignSource struct {
	Source   string `json:"source,omitempty"`
	Medium   string `json:"medium,omitempty"`
	Campaign string `json:"campaign,omitempty"`
	Content  string `json:"content,omitempty"`
	Term     string `json:"term,omitempty"`
}

// SubmitDesignPartnerRequest is the applicant-supplied payload. IdempotencyKey
// derives the application ID, so a replayed submission folds onto the same entity.
type SubmitDesignPartnerRequest struct {
	Name              string                      `json:"name"`
	Email             string                      `json:"email"`
	Project           string                      `json:"project"`
	AgentUseCase      string                      `json:"agent_use_case"`
	MemoryProblem     string                      `json:"memory_problem"`
	Timeline          string                      `json:"timeline"`
	Consent           bool                        `json:"consent"`
	IdempotencyKey    string                      `json:"idempotency_key"`
	CampaignSource    DesignPartnerCampaignSource `json:"campaign_source"`
	TurnstileResponse string                      `json:"cf_turnstile_response,omitempty"`
}

// DesignPartnerStatusChange is one entry of the review audit trail.
type DesignPartnerStatusChange struct {
	Status    string `json:"status"`
	ChangedAt string `json:"changed_at"`
	Actor     string `json:"actor,omitempty"`
	Note      string `json:"note,omitempty"`
}

// DesignPartnerApplication is the projection folded from an entity's event
// stream — never a stored row.
type DesignPartnerApplication struct {
	ID             string                      `json:"id"`
	Name           string                      `json:"name"`
	Email          string                      `json:"email"`
	Project        string                      `json:"project"`
	AgentUseCase   string                      `json:"agent_use_case"`
	MemoryProblem  string                      `json:"memory_problem"`
	Timeline       string                      `json:"timeline"`
	Status         string                      `json:"status"`
	SubmittedAt    string                      `json:"submitted_at"`
	ConsentVersion string                      `json:"consent_version"`
	CampaignSource DesignPartnerCampaignSource `json:"campaign_source"`
	RetentionUntil string                      `json:"retention_until,omitempty"`
	StatusHistory  []DesignPartnerStatusChange `json:"status_history"`
}

// UpdateDesignPartnerStatusRequest is a reviewer decision. ApplicationID and
// Actor come from the route and the session, never from the request body.
type UpdateDesignPartnerStatusRequest struct {
	ApplicationID string `json:"-"`
	Status        string `json:"status"`
	Actor         string `json:"-"`
	Note          string `json:"note,omitempty"`
}

// Submit validates an application and appends it at version 0, so a replayed
// idempotency key loses the version race and is reported as success without
// writing a second event.
func (uc *DesignPartnerUseCase) Submit(ctx context.Context, req SubmitDesignPartnerRequest) (*DesignPartnerApplication, error) {
	if uc == nil || uc.core == nil {
		return nil, ErrDesignPartnerUnavailable
	}
	if err := validateDesignPartnerSubmission(&req); err != nil {
		return nil, err
	}

	applicationID := designPartnerApplicationID(req.IdempotencyKey)
	submittedAt := uc.now().UTC().Format(time.RFC3339Nano)
	var firstVersion uint64
	payload := map[string]any{
		"application_id":  applicationID,
		"name":            req.Name,
		"email":           req.Email,
		"project":         req.Project,
		"agent_use_case":  req.AgentUseCase,
		"memory_problem":  req.MemoryProblem,
		"timeline":        req.Timeline,
		"status":          DesignPartnerStatusNew,
		"submitted_at":    submittedAt,
		"consent_version": DesignPartnerConsentVersion,
		"campaign_source": campaignSourcePayload(req.CampaignSource),
	}

	_, err := uc.core.IngestEvent(ctx, clients.IngestEventRequest{
		EventType:       DesignPartnerSubmittedEventType,
		EntityID:        designPartnerEntityID(applicationID),
		TenantID:        DesignPartnerTenant,
		Payload:         payload,
		Metadata:        map[string]any{"idempotency_key": req.IdempotencyKey},
		ExpectedVersion: &firstVersion,
	})
	if err != nil && !errors.Is(err, clients.ErrVersionConflict) {
		return nil, fmt.Errorf("%w: write application", ErrDesignPartnerUnavailable)
	}

	return &DesignPartnerApplication{
		ID: applicationID, Name: req.Name, Email: req.Email, Project: req.Project,
		AgentUseCase: req.AgentUseCase, MemoryProblem: req.MemoryProblem, Timeline: req.Timeline,
		Status: DesignPartnerStatusNew, SubmittedAt: submittedAt, ConsentVersion: DesignPartnerConsentVersion,
		CampaignSource: req.CampaignSource,
		StatusHistory:  []DesignPartnerStatusChange{{Status: DesignPartnerStatusNew, ChangedAt: submittedAt}},
	}, nil
}

// List folds every application in the tenant. An empty status returns all of
// them; any other value must be one of the DesignPartnerStatus constants.
func (uc *DesignPartnerUseCase) List(ctx context.Context, status string) ([]DesignPartnerApplication, error) {
	if uc == nil || uc.core == nil {
		return nil, ErrDesignPartnerUnavailable
	}
	status = strings.TrimSpace(strings.ToLower(status))
	if status != "" && !validDesignPartnerStatus(status) {
		return nil, fmt.Errorf("%w: unsupported status", ErrDesignPartnerInvalidInput)
	}

	resp, err := uc.core.QueryEvents(ctx, clients.QueryEventsRequest{
		TenantID:        DesignPartnerTenant,
		EventTypePrefix: "design_partner.",
		Limit:           1000,
		Order:           "asc",
	})
	if err != nil {
		return nil, fmt.Errorf("%w: read applications", ErrDesignPartnerUnavailable)
	}
	applications := projectDesignPartnerApplications(eventsFromResponse(resp))
	if status == "" {
		return applications, nil
	}
	filtered := make([]DesignPartnerApplication, 0, len(applications))
	for i := range applications {
		if applications[i].Status == status {
			filtered = append(filtered, applications[i])
		}
	}
	return filtered, nil
}

// UpdateStatus appends a review decision and returns the refolded application.
// The write is version-checked, so a concurrent decision fails rather than
// silently overwriting the other reviewer's.
func (uc *DesignPartnerUseCase) UpdateStatus(ctx context.Context, req UpdateDesignPartnerStatusRequest) (*DesignPartnerApplication, error) {
	if uc == nil || uc.core == nil {
		return nil, ErrDesignPartnerUnavailable
	}
	req.ApplicationID = strings.TrimSpace(req.ApplicationID)
	req.Status = strings.TrimSpace(strings.ToLower(req.Status))
	req.Actor = strings.TrimSpace(req.Actor)
	req.Note = strings.TrimSpace(req.Note)
	if !applicationIDPattern.MatchString(req.ApplicationID) || !validDesignPartnerStatus(req.Status) || len(req.Note) > 500 {
		return nil, fmt.Errorf("%w: invalid status change", ErrDesignPartnerInvalidInput)
	}

	resp, err := uc.core.QueryEvents(ctx, clients.QueryEventsRequest{
		TenantID: DesignPartnerTenant,
		EntityID: designPartnerEntityID(req.ApplicationID),
		Limit:    100,
		Order:    "asc",
	})
	if err != nil {
		return nil, fmt.Errorf("%w: read application", ErrDesignPartnerUnavailable)
	}
	events := eventsFromResponse(resp)
	applications := projectDesignPartnerApplications(events)
	if len(applications) == 0 {
		return nil, ErrDesignPartnerNotFound
	}
	application := applications[0]
	if application.Status == req.Status {
		return &application, nil
	}

	changedAt := uc.now().UTC()
	payload := map[string]any{
		"application_id": req.ApplicationID,
		"status":         req.Status,
		"actor":          req.Actor,
		"changed_at":     changedAt.Format(time.RFC3339Nano),
	}
	if req.Note != "" {
		payload["note"] = req.Note
	}
	if retentionUntil := designPartnerRetentionUntil(req.Status, changedAt); !retentionUntil.IsZero() {
		payload["retention_until"] = retentionUntil.Format(time.RFC3339Nano)
	}

	expectedVersion := uint64(len(events))
	// TotalCount comes off the wire; the >= 0 guards the unsigned conversion.
	if resp != nil && resp.TotalCount >= 0 && resp.TotalCount > len(events) {
		expectedVersion = uint64(resp.TotalCount)
	}
	_, err = uc.core.IngestEvent(ctx, clients.IngestEventRequest{
		EventType:       DesignPartnerStatusEventType,
		EntityID:        designPartnerEntityID(req.ApplicationID),
		TenantID:        DesignPartnerTenant,
		Payload:         payload,
		ExpectedVersion: &expectedVersion,
	})
	if err != nil {
		return nil, fmt.Errorf("%w: write status", ErrDesignPartnerUnavailable)
	}

	events = append(events, clients.EventEntry{
		EventType: DesignPartnerStatusEventType,
		EntityID:  designPartnerEntityID(req.ApplicationID),
		Timestamp: changedAt.Format(time.RFC3339Nano),
		Payload:   payload,
	})
	updated := projectDesignPartnerApplications(events)
	if len(updated) == 0 {
		return nil, ErrDesignPartnerNotFound
	}
	return &updated[0], nil
}

func validateDesignPartnerSubmission(req *SubmitDesignPartnerRequest) error {
	req.Name = strings.TrimSpace(req.Name)
	req.Email = strings.ToLower(strings.TrimSpace(req.Email))
	req.Project = strings.TrimSpace(req.Project)
	req.AgentUseCase = strings.TrimSpace(req.AgentUseCase)
	req.MemoryProblem = strings.TrimSpace(req.MemoryProblem)
	req.Timeline = strings.TrimSpace(strings.ToLower(req.Timeline))
	req.IdempotencyKey = strings.TrimSpace(req.IdempotencyKey)
	req.CampaignSource = normalizeCampaignSource(req.CampaignSource)

	if len(req.Name) < 2 || len(req.Name) > 80 {
		return fmt.Errorf("%w: name must be 2-80 characters", ErrDesignPartnerInvalidInput)
	}
	address, err := mail.ParseAddress(req.Email)
	if err != nil || address.Address != req.Email || len(req.Email) > 254 {
		return fmt.Errorf("%w: valid work email required", ErrDesignPartnerInvalidInput)
	}
	if len(req.Project) < 2 || len(req.Project) > 120 {
		return fmt.Errorf("%w: project must be 2-120 characters", ErrDesignPartnerInvalidInput)
	}
	if len(req.AgentUseCase) < 30 || len(req.AgentUseCase) > 1000 {
		return fmt.Errorf("%w: agent use case must be 30-1000 characters", ErrDesignPartnerInvalidInput)
	}
	if len(req.MemoryProblem) < 30 || len(req.MemoryProblem) > 1000 {
		return fmt.Errorf("%w: memory problem must be 30-1000 characters", ErrDesignPartnerInvalidInput)
	}
	if !validDesignPartnerTimeline(req.Timeline) {
		return fmt.Errorf("%w: unsupported integration timeline", ErrDesignPartnerInvalidInput)
	}
	if !req.Consent {
		return fmt.Errorf("%w: consent required", ErrDesignPartnerInvalidInput)
	}
	if !idempotencyKeyPattern.MatchString(req.IdempotencyKey) {
		return fmt.Errorf("%w: invalid idempotency key", ErrDesignPartnerInvalidInput)
	}
	if campaignSourceTooLong(req.CampaignSource) {
		return fmt.Errorf("%w: campaign source value too long", ErrDesignPartnerInvalidInput)
	}
	return nil
}

func validDesignPartnerTimeline(value string) bool {
	switch value {
	case "ready_now", "within_30_days", "within_60_days", "exploring":
		return true
	default:
		return false
	}
}

func validDesignPartnerStatus(value string) bool {
	switch value {
	case DesignPartnerStatusNew,
		DesignPartnerStatusReviewing,
		DesignPartnerStatusAccepted,
		DesignPartnerStatusWaitlisted,
		DesignPartnerStatusRejected:
		return true
	default:
		return false
	}
}

func designPartnerApplicationID(idempotencyKey string) string {
	hash := sha256.Sum256([]byte("design-partner|" + idempotencyKey))
	return hex.EncodeToString(hash[:16])
}

func designPartnerEntityID(applicationID string) string {
	return "design-partner:" + applicationID
}

func designPartnerRetentionUntil(status string, changedAt time.Time) time.Time {
	switch status {
	case DesignPartnerStatusAccepted:
		return changedAt.AddDate(0, 0, 150) // 60-day program + 90 days.
	case DesignPartnerStatusWaitlisted, DesignPartnerStatusRejected:
		return changedAt.AddDate(0, 0, 90)
	default:
		return time.Time{}
	}
}

func normalizeCampaignSource(source DesignPartnerCampaignSource) DesignPartnerCampaignSource {
	source.Source = strings.TrimSpace(source.Source)
	source.Medium = strings.TrimSpace(source.Medium)
	source.Campaign = strings.TrimSpace(source.Campaign)
	source.Content = strings.TrimSpace(source.Content)
	source.Term = strings.TrimSpace(source.Term)
	return source
}

func campaignSourceTooLong(source DesignPartnerCampaignSource) bool {
	return len(source.Source) > 100 || len(source.Medium) > 100 || len(source.Campaign) > 100 ||
		len(source.Content) > 100 || len(source.Term) > 100
}

func campaignSourcePayload(source DesignPartnerCampaignSource) map[string]any {
	return map[string]any{
		"source": source.Source, "medium": source.Medium, "campaign": source.Campaign,
		"content": source.Content, "term": source.Term,
	}
}

func eventsFromResponse(resp *clients.QueryEventsResponse) []clients.EventEntry {
	if resp == nil {
		return nil
	}
	return resp.Events
}

func projectDesignPartnerApplications(events []clients.EventEntry) []DesignPartnerApplication {
	byID := make(map[string]*DesignPartnerApplication)
	for _, event := range events {
		applicationID := payloadString(event.Payload, "application_id")
		if applicationID == "" {
			continue
		}
		switch event.EventType {
		case DesignPartnerSubmittedEventType:
			if _, exists := byID[applicationID]; exists {
				continue
			}
			submittedAt := payloadString(event.Payload, "submitted_at")
			if submittedAt == "" {
				submittedAt = event.Timestamp
			}
			app := &DesignPartnerApplication{
				ID: applicationID, Name: payloadString(event.Payload, "name"),
				Email: payloadString(event.Payload, "email"), Project: payloadString(event.Payload, "project"),
				AgentUseCase:  payloadString(event.Payload, "agent_use_case"),
				MemoryProblem: payloadString(event.Payload, "memory_problem"),
				Timeline:      payloadString(event.Payload, "timeline"), Status: DesignPartnerStatusNew,
				SubmittedAt: submittedAt, ConsentVersion: payloadString(event.Payload, "consent_version"),
				CampaignSource: campaignSourceFromPayload(event.Payload["campaign_source"]),
				StatusHistory:  []DesignPartnerStatusChange{{Status: DesignPartnerStatusNew, ChangedAt: submittedAt}},
			}
			byID[applicationID] = app
		case DesignPartnerStatusEventType:
			app := byID[applicationID]
			if app == nil {
				continue
			}
			changedAt := payloadString(event.Payload, "changed_at")
			if changedAt == "" {
				changedAt = event.Timestamp
			}
			app.Status = payloadString(event.Payload, "status")
			app.RetentionUntil = payloadString(event.Payload, "retention_until")
			app.StatusHistory = append(app.StatusHistory, DesignPartnerStatusChange{
				Status: app.Status, ChangedAt: changedAt, Actor: payloadString(event.Payload, "actor"),
				Note: payloadString(event.Payload, "note"),
			})
		}
	}

	result := make([]DesignPartnerApplication, 0, len(byID))
	for _, application := range byID {
		result = append(result, *application)
	}
	sort.Slice(result, func(i, j int) bool { return result[i].SubmittedAt > result[j].SubmittedAt })
	return result
}

func payloadString(payload map[string]any, key string) string {
	if value, ok := payload[key].(string); ok {
		return value
	}
	return ""
}

func campaignSourceFromPayload(value any) DesignPartnerCampaignSource {
	values, ok := value.(map[string]any)
	if !ok {
		return DesignPartnerCampaignSource{}
	}
	return DesignPartnerCampaignSource{
		Source: payloadString(values, "source"), Medium: payloadString(values, "medium"),
		Campaign: payloadString(values, "campaign"), Content: payloadString(values, "content"),
		Term: payloadString(values, "term"),
	}
}
