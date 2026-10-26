package allsource

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strconv"
	"strings"
)

// FeedCursor is a position in the tenant's event feed. Positions are only
// comparable within one epoch.
type FeedCursor struct {
	Epoch    int64 `json:"epoch"`
	Position int64 `json:"position"`
}

// EntityStateOptions configures EntityState.
type EntityStateOptions struct {
	// AsOf is an RFC 3339 timestamp; empty means now.
	AsOf string
	// History asks for the history[] array as well as the folded state.
	History bool
	// IfNoneMatch is an ETag from an earlier EntityState.
	IfNoneMatch string
}

// EntityState is one entity's state folded from this tenant's events.
type EntityState struct {
	EntityID     string           `json:"entity_id"`
	CurrentState map[string]any   `json:"current_state"`
	EventCount   int64            `json:"event_count"`
	LastUpdated  *string          `json:"last_updated"`
	AsOf         *string          `json:"as_of"`
	Version      int64            `json:"version"`
	History      []map[string]any `json:"history,omitempty"`
	ETag         string           `json:"-"`
}

// EntityState reads one entity's current state without its history unless
// opts.History is set. With opts.IfNoneMatch an unchanged state returns a
// *NotModifiedError (errors.Is(err, ErrNotModified)).
func (c *Client) EntityState(ctx context.Context, entityID string, opts EntityStateOptions) (*EntityState, error) {
	params := url.Values{}
	if opts.AsOf != "" {
		params.Set("as_of", opts.AsOf)
	}
	if opts.History {
		params.Set("history", "true")
	}
	path := withQuery("/api/v1/entities/"+url.PathEscape(entityID)+"/state", params)

	var st EntityState
	etag, err := c.getJSON(ctx, path, opts.IfNoneMatch, "entity state", &st)
	if err != nil {
		return nil, err
	}
	st.ETag = etag
	return &st, nil
}

// FeedOptions configures one Feed page.
type FeedOptions struct {
	// Cursor is sent as epoch and position together, or neither when nil.
	Cursor *FeedCursor
	// Prefixes filters by event-type prefix: at most 8, each at most 128 bytes.
	Prefixes []string
	EntityID string
	// Limit defaults to 100 on the server when zero.
	Limit int
}

// FeedPage is one page of the tenant's event feed.
type FeedPage struct {
	TenantID  string     `json:"tenant_id"`
	Cursor    FeedCursor `json:"cursor"`
	Head      FeedCursor `json:"head"`
	Events    []Event    `json:"events"`
	Positions []int64    `json:"positions"`
	CaughtUp  bool       `json:"caught_up"`
}

// Next is the cursor to pass to the following Feed call: the last element of
// Positions, or the request cursor when the page is empty.
func (p *FeedPage) Next() FeedCursor {
	if len(p.Positions) == 0 {
		return p.Cursor
	}
	return FeedCursor{Epoch: p.Cursor.Epoch, Position: p.Positions[len(p.Positions)-1]}
}

// Feed reads one page of the tenant's event feed. A changed feed generation
// returns an *EpochChangedError.
func (c *Client) Feed(ctx context.Context, opts FeedOptions) (*FeedPage, error) {
	params := url.Values{}
	if opts.Cursor != nil {
		params.Set("epoch", strconv.FormatInt(opts.Cursor.Epoch, 10))
		params.Set("position", strconv.FormatInt(opts.Cursor.Position, 10))
	}
	if opts.Limit > 0 {
		params.Set("limit", strconv.Itoa(opts.Limit))
	}
	if len(opts.Prefixes) > 0 {
		params.Set("prefixes", strings.Join(opts.Prefixes, ","))
	}
	if opts.EntityID != "" {
		params.Set("entity_id", opts.EntityID)
	}

	respData, _, err := c.do(ctx, http.MethodGet, withQuery("/api/v1/event-feed", params), nil)
	if err != nil {
		if apiErr, ok := apiErrorWith(err, http.StatusConflict, "version_conflict"); ok {
			expected, current := versionConflictFields(apiErr)
			return nil, &EpochChangedError{ExpectedEpoch: expected, CurrentEpoch: current, API: apiErr}
		}
		return nil, err
	}

	var page FeedPage
	if err := json.Unmarshal(respData, &page); err != nil {
		return nil, fmt.Errorf("decode event feed: %w", err)
	}
	return &page, nil
}

// ReadModel is a handle on one named read model.
type ReadModel struct {
	client *Client
	name   string
}

// ReadModel returns a handle on the read model called name.
func (c *Client) ReadModel(name string) *ReadModel {
	return &ReadModel{client: c, name: name}
}

// ReadModelPageOptions configures one read-model page.
type ReadModelPageOptions struct {
	Prefix string
	// After is the Next value of the previous page.
	After string
	// Limit is 1..500; the server defaults to 100 when zero.
	Limit int
	// Revision is a ContentRevision from an earlier page; an unchanged model
	// answers Unchanged with no rows.
	Revision *int64
}

// ReadModelPage is one page of read-model rows.
type ReadModelPage struct {
	Revision        int64          `json:"revision"`
	ContentRevision int64          `json:"content_revision"`
	Checkpoint      *FeedCursor    `json:"checkpoint"`
	Ready           bool           `json:"ready"`
	Unchanged       bool           `json:"unchanged"`
	Rows            map[string]any `json:"rows"`
	Next            *string        `json:"next"`
}

// Page reads one page of rows. When Unchanged is set, the empty Rows mean
// "nothing changed", never "every row was deleted".
func (m *ReadModel) Page(ctx context.Context, opts ReadModelPageOptions) (*ReadModelPage, error) {
	params := url.Values{}
	if opts.Prefix != "" {
		params.Set("prefix", opts.Prefix)
	}
	if opts.After != "" {
		params.Set("after", opts.After)
	}
	if opts.Limit > 0 {
		params.Set("limit", strconv.Itoa(opts.Limit))
	}
	if opts.Revision != nil {
		params.Set("revision", strconv.FormatInt(*opts.Revision, 10))
	}

	respData, _, err := m.client.do(ctx, http.MethodGet, withQuery(m.path(), params), nil)
	if err != nil {
		return nil, err
	}

	var page ReadModelPage
	if err := json.Unmarshal(respData, &page); err != nil {
		return nil, fmt.Errorf("decode read model page: %w", err)
	}
	return &page, nil
}

// ReadModelCommit is one atomic write to a read model.
type ReadModelCommit struct {
	ExpectedRevision int64      `json:"expected_revision"`
	Checkpoint       FeedCursor `json:"checkpoint"`
	Ready            bool       `json:"ready"`
	Reset            bool       `json:"reset"`
	// Changes maps row keys to their new value; a nil value deletes the row.
	Changes map[string]any `json:"changes"`
}

// Commit writes commit and returns the new revision. A stale
// ExpectedRevision returns a *VersionConflictError.
func (m *ReadModel) Commit(ctx context.Context, commit ReadModelCommit) (int64, error) {
	if commit.Changes == nil {
		commit.Changes = map[string]any{}
	}
	respData, _, err := m.client.do(ctx, http.MethodPut, m.path(), commit)
	if err != nil {
		if apiErr, ok := apiErrorWith(err, http.StatusConflict, "version_conflict"); ok {
			expected, current := versionConflictFields(apiErr)
			return 0, &VersionConflictError{ExpectedRevision: expected, CurrentRevision: current, API: apiErr}
		}
		return 0, err
	}

	var resp struct {
		Revision int64 `json:"revision"`
	}
	if err := json.Unmarshal(respData, &resp); err != nil {
		return 0, fmt.Errorf("decode read model commit: %w", err)
	}
	return resp.Revision, nil
}

func (m *ReadModel) path() string {
	return "/api/v1/read-models/" + url.PathEscape(m.name)
}
