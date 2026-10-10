package usecases

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"net/url"
	"reflect"
	"regexp"
	"sort"
	"strings"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

const partnershipTenant = "admin-partnerships"
const partnershipEvent = "partnership.record_saved"

// statusDoNotContact suppresses a prospect. Removing it is deliberately not
// an edit a record save can make.
const statusDoNotContact = "do_not_contact"

var (
	// ErrPartnershipInvalid reports a record that failed validation.
	ErrPartnershipInvalid = errors.New("partnership: invalid record")
	// ErrPartnershipConflict reports a stale expected revision.
	ErrPartnershipConflict = errors.New("partnership: record changed; reload before saving")
	// ErrPartnershipUnavailable reports that Core could not be reached.
	ErrPartnershipUnavailable = errors.New("partnership: storage unavailable")
	partnershipHost           = regexp.MustCompile(`^[a-z0-9][a-z0-9.-]{1,250}[a-z0-9]$`)
)

// PartnershipSource cites one piece of dated evidence behind a record.
type PartnershipSource struct {
	URL       string `json:"url"`
	Title     string `json:"title"`
	Evidence  string `json:"evidence"`
	CheckedAt string `json:"checked_at"`
}

// PartnershipScore describes a dated evidence packet, never a likelihood of
// winning work.
type PartnershipScore struct {
	Model      string  `json:"model"`
	RunAt      string  `json:"run_at"`
	Fit        float64 `json:"fit"`
	Leverage   float64 `json:"leverage"`
	Access     float64 `json:"access"`
	PaidDemand float64 `json:"paid_demand"`
	Rationale  string  `json:"rationale"`
}

// PartnershipMessage records one outbound or inbound contact on a record.
type PartnershipMessage struct {
	ID           string `json:"id"`
	Channel      string `json:"channel"`
	Direction    string `json:"direction"`
	Outcome      string `json:"outcome"`
	Destination  string `json:"destination"`
	Subject      string `json:"subject"`
	Body         string `json:"body"`
	OccurredAt   string `json:"occurred_at"`
	Verification string `json:"verification"`
	ApprovalNote string `json:"approval_note"`
}

// PartnershipRecord is the current state of one partnership prospect.
type PartnershipRecord struct {
	ID             string               `json:"id"`
	Organization   string               `json:"organization"`
	Kind           string               `json:"kind"`
	Geography      string               `json:"geography"`
	Website        string               `json:"website"`
	Status         string               `json:"status"`
	Angle          string               `json:"angle"`
	ContactRoute   string               `json:"contact_route"`
	NextAction     string               `json:"next_action"`
	NextActionAt   string               `json:"next_action_at"`
	Notes          string               `json:"notes"`
	Limitations    string               `json:"limitations"`
	ReplyCheckedAt string               `json:"reply_checked_at"`
	Sources        []PartnershipSource  `json:"sources"`
	Score          *PartnershipScore    `json:"score"`
	Messages       []PartnershipMessage `json:"messages"`
}

// PartnershipRevision pairs a record snapshot with the revision that produced
// it, so a caller can fence its next write.
type PartnershipRevision struct {
	Record   PartnershipRecord `json:"record"`
	Revision uint64            `json:"revision"`
	SavedAt  string            `json:"saved_at"`
	Actor    string            `json:"actor"`
}

// SavePartnershipRequest carries a record plus the revision the caller expects
// to be replacing. A nil ExpectedRevision means "create".
type SavePartnershipRequest struct {
	Record           PartnershipRecord `json:"record"`
	ExpectedRevision *uint64           `json:"expected_revision"`
}

// PartnershipsUseCase reads and writes partnership records through Core.
type PartnershipsUseCase struct {
	core designPartnerCore
}

// NewPartnershipsUseCase builds a use case over the given Core client.
func NewPartnershipsUseCase(core clients.CoreClient) *PartnershipsUseCase {
	return &PartnershipsUseCase{core: core}
}

// Each revision is a complete snapshot, but lives in a durable, append-only
// entity stream. Partial event scans must never look like an empty/new record.
func (uc *PartnershipsUseCase) read(ctx context.Context, id string) ([]PartnershipRevision, error) {
	if uc == nil || uc.core == nil {
		return nil, ErrPartnershipUnavailable
	}
	out := make([]PartnershipRevision, 0)
	const pageSize = 500
	for offset := 0; offset < 50000; offset += pageSize {
		entity := ""
		if id != "" {
			entity = "partnership:" + id
		}
		resp, err := uc.core.QueryEvents(ctx, clients.QueryEventsRequest{
			TenantID: partnershipTenant, EventType: partnershipEvent, EntityID: entity,
			Order: "asc", Limit: pageSize, Offset: offset,
		})
		if err != nil || resp == nil {
			return nil, ErrPartnershipUnavailable
		}
		for _, event := range resp.Events {
			if event.EventType != partnershipEvent {
				continue
			}
			encoded, err := json.Marshal(event.Payload)
			if err != nil {
				return nil, ErrPartnershipUnavailable
			}
			var revision PartnershipRevision
			if json.Unmarshal(encoded, &revision) != nil || revision.Revision == 0 || revision.Record.ID == "" || event.EntityID != "partnership:"+revision.Record.ID {
				return nil, ErrPartnershipUnavailable
			}
			out = append(out, revision)
		}
		if len(resp.Events) < pageSize {
			if resp.TotalCount > offset+len(resp.Events) {
				return nil, ErrPartnershipUnavailable
			}
			return out, nil
		}
	}
	return nil, ErrPartnershipUnavailable
}

// List returns the latest revision of every partnership record.
func (uc *PartnershipsUseCase) List(ctx context.Context) ([]PartnershipRevision, error) {
	revisions, err := uc.read(ctx, "")
	if err != nil {
		return nil, err
	}
	latest := make(map[string]PartnershipRevision)
	for i := range revisions {
		rev := &revisions[i]
		if rev.Revision > latest[rev.Record.ID].Revision {
			latest[rev.Record.ID] = *rev
		}
	}
	out := make([]PartnershipRevision, 0, len(latest))
	for id := range latest {
		out = append(out, latest[id])
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Record.Organization < out[j].Record.Organization })
	return out, nil
}

// History returns every revision of one record, oldest first.
func (uc *PartnershipsUseCase) History(ctx context.Context, id string) ([]PartnershipRevision, error) {
	if !partnershipHost.MatchString(id) {
		return nil, ErrPartnershipInvalid
	}
	return uc.read(ctx, id)
}

// Save appends a revision, rejecting the write unless ExpectedRevision matches
// the stored one.
func (uc *PartnershipsUseCase) Save(ctx context.Context, req SavePartnershipRequest, actor string) (*PartnershipRevision, error) {
	if req.ExpectedRevision == nil {
		return nil, fmt.Errorf("%w: expected_revision is required", ErrPartnershipInvalid)
	}
	if err := validatePartnership(&req.Record); err != nil {
		return nil, err
	}
	history, err := uc.read(ctx, req.Record.ID)
	if err != nil {
		return nil, err
	}
	var current PartnershipRevision
	for i := range history {
		if history[i].Revision > current.Revision {
			current = history[i]
		}
	}
	if current.Revision != *req.ExpectedRevision {
		return nil, ErrPartnershipConflict
	}
	if current.Revision > 0 {
		// Evidence of an external interaction cannot disappear on a CRM edit.
		for i := range current.Record.Messages {
			old := &current.Record.Messages[i]
			if old.Outcome == "draft" {
				continue
			}
			found := false
			for j := range req.Record.Messages {
				next := &req.Record.Messages[j]
				if old.ID == next.ID && reflect.DeepEqual(old, next) {
					found = true
					break
				}
			}
			if !found {
				return nil, fmt.Errorf("%w: recorded interactions are immutable; append a correction", ErrPartnershipInvalid)
			}
		}
		if current.Record.Status == statusDoNotContact && req.Record.Status != statusDoNotContact {
			return nil, fmt.Errorf("%w: suppression cannot be removed through a record edit", ErrPartnershipInvalid)
		}
		if reflect.DeepEqual(current.Record, req.Record) {
			return &current, nil
		}
	}
	saved := PartnershipRevision{Record: req.Record, Revision: current.Revision + 1, Actor: actor, SavedAt: time.Now().UTC().Format(time.RFC3339Nano)}
	encoded, err := json.Marshal(saved)
	if err != nil {
		return nil, ErrPartnershipInvalid
	}
	var payload map[string]any
	if err = json.Unmarshal(encoded, &payload); err != nil {
		return nil, ErrPartnershipInvalid
	}
	_, err = uc.core.IngestEvent(ctx, clients.IngestEventRequest{
		TenantID: partnershipTenant, EntityID: "partnership:" + req.Record.ID,
		EventType: partnershipEvent, Payload: payload, ExpectedVersion: req.ExpectedRevision,
	})
	if errors.Is(err, clients.ErrVersionConflict) {
		return nil, ErrPartnershipConflict
	}
	if err != nil {
		return nil, ErrPartnershipUnavailable
	}
	return &saved, nil
}

func partnershipURL(value string) bool {
	u, err := url.Parse(value)
	return err == nil && (u.Scheme == "https" || u.Scheme == "http") && u.Hostname() != "" && u.User == nil
}

func partnershipDate(value string, optional bool) bool {
	if optional && value == "" {
		return true
	}
	_, err := time.Parse(time.RFC3339, value)
	return err == nil
}

func oneOf(value string, options ...string) bool {
	for _, option := range options {
		if value == option {
			return true
		}
	}
	return false
}

func validatePartnership(r *PartnershipRecord) error {
	invalid := func(message string) error { return fmt.Errorf("%w: %s", ErrPartnershipInvalid, message) }
	if !partnershipURL(r.Website) {
		return invalid("valid organisation website required")
	}
	// Unreachable while partnershipURL above guards it; checked so a nil u
	// cannot reach Hostname() if that guard moves.
	u, err := url.Parse(r.Website)
	if err != nil {
		return invalid("valid organisation website required")
	}
	id := strings.TrimPrefix(strings.ToLower(u.Hostname()), "www.")
	if !partnershipHost.MatchString(id) || !strings.Contains(id, ".") || (r.ID != "" && r.ID != id) {
		return invalid("id must match the canonical website hostname")
	}
	r.ID = id
	if strings.TrimSpace(r.Organization) == "" || len(r.Organization) > 160 {
		return invalid("organisation name required (maximum 160 characters)")
	}
	if !oneOf(r.Kind, "vc", "family_office", "accelerator", "corporate", "community", "other") {
		return invalid("unsupported organisation type")
	}
	if !oneOf(r.Status, "research", "ready", "awaiting_reply", "engaged", "pilot", "won", "parked", statusDoNotContact) {
		return invalid("unsupported pipeline status")
	}
	if len(r.Geography) > 250 || len(r.ContactRoute) > 2000 || len(r.Angle) > 8000 || len(r.Notes) > 20000 || len(r.Limitations) > 8000 || len(r.NextAction) > 2000 {
		return invalid("record text is too long")
	}
	if !partnershipDate(r.NextActionAt, true) || !partnershipDate(r.ReplyCheckedAt, true) {
		return invalid("dates must be RFC3339 timestamps")
	}
	if len(r.Sources) > 50 || len(r.Messages) > 200 {
		return invalid("limit is 50 sources and 200 messages per organisation")
	}
	if r.Sources == nil {
		r.Sources = []PartnershipSource{}
	}
	if r.Messages == nil {
		r.Messages = []PartnershipMessage{}
	}
	for i := range r.Sources {
		s := &r.Sources[i]
		if !partnershipURL(s.URL) || s.Title == "" || s.Evidence == "" || len(s.Title) > 300 || len(s.Evidence) > 8000 || !partnershipDate(s.CheckedAt, false) {
			return invalid("each source needs a URL, title, evidence and checked_at timestamp")
		}
	}
	if r.Score != nil {
		s := r.Score
		if s.Model == "" || len(s.Model) > 100 || !partnershipDate(s.RunAt, false) || s.Rationale == "" || len(s.Rationale) > 8000 {
			return invalid("score requires model, run_at and rationale")
		}
		for _, n := range []float64{s.Fit, s.Leverage, s.Access, s.PaidDemand} {
			if math.IsNaN(n) || math.IsInf(n, 0) || n < 0 || n > 3 {
				return invalid("score dimensions must be 0–3")
			}
		}
	}
	seen := make(map[string]bool)
	for i := range r.Messages {
		m := &r.Messages[i]
		if m.ID == "" || len(m.ID) > 160 || seen[m.ID] {
			return invalid("message ids must be non-empty and unique")
		}
		seen[m.ID] = true
		if !oneOf(m.Channel, "email", "linkedin", "x", "form", "other") || !oneOf(m.Direction, "outbound", "inbound") || !oneOf(m.Outcome, "draft", "sent", "received", "failed", "unknown") {
			return invalid("unsupported message channel, direction or outcome")
		}
		if (m.Outcome == "received") != (m.Direction == "inbound") {
			return invalid("inbound messages must use received outcome")
		}
		if strings.TrimSpace(m.Body) == "" || m.Destination == "" || len(m.Body) > 20000 || len(m.Destination) > 2000 || len(m.Subject) > 300 || len(m.Verification) > 4000 || len(m.ApprovalNote) > 2000 {
			return invalid("message body and destination required within size limits")
		}
		if !partnershipDate(m.OccurredAt, m.Outcome == "draft") {
			return invalid("recorded messages require an occurred_at timestamp")
		}
		if (m.Outcome == "sent" || m.Outcome == "received") && strings.TrimSpace(m.Verification) == "" {
			return invalid("sent/received needs verification evidence")
		}
	}
	return nil
}
