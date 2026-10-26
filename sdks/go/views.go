package allsource

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"iter"
	"math"
	"net/http"
	"net/url"
	"strconv"
	"time"
)

// maxViewWait is the longest long-poll Core holds a changes request.
const maxViewWait = 20 * time.Second

// ViewSource selects the events a view folds.
type ViewSource struct {
	Prefixes []string `json:"prefixes"`
}

// ViewStatusOp sets Field from the event type's last segment (From "suffix")
// or from a value expression, optionally restricted to listed transitions.
type ViewStatusOp struct {
	Field       string              `json:"field"`
	From        any                 `json:"from"`
	Transitions map[string][]string `json:"transitions,omitempty"`
}

// ViewOp is one reducer operation. Values in Set, Max, Min, Append and
// Remove are expressions: "payload.<path>", "entity_id", "event_type",
// "timestamp", "day:timestamp", or ViewLiteral(v).
type ViewOp struct {
	On        string           `json:"on"`
	When      map[string]any   `json:"when,omitempty"`
	Set       map[string]any   `json:"set,omitempty"`
	Unset     []string         `json:"unset,omitempty"`
	Delete    bool             `json:"delete,omitempty"`
	Increment map[string]int64 `json:"increment,omitempty"`
	Max       map[string]any   `json:"max,omitempty"`
	Min       map[string]any   `json:"min,omitempty"`
	Append    map[string]any   `json:"append,omitempty"`
	Remove    map[string]any   `json:"remove,omitempty"`
	Status    *ViewStatusOp    `json:"status,omitempty"`
}

// ViewLiteral wraps a JSON value so a view expression uses it verbatim.
func ViewLiteral(v any) map[string]any {
	return map[string]any{"literal": v}
}

// ViewDefinition is a declarative read model. Core validates and stores it;
// the Query Service folds it from the event feed.
type ViewDefinition struct {
	Source ViewSource `json:"source"`
	Key    string     `json:"key"`
	Ops    []ViewOp   `json:"ops"`
}

// View is a stored view definition and its fold progress.
type View struct {
	View       string          `json:"view"`
	Definition *ViewDefinition `json:"definition"`
	Generation int64           `json:"generation"`
	Revision   int64           `json:"revision"`
	Checkpoint *FeedCursor     `json:"checkpoint"`
	Ready      bool            `json:"ready"`
}

// ViewSummary is one entry of ListViews.
type ViewSummary struct {
	View       string `json:"view"`
	Generation int64  `json:"generation"`
	Revision   int64  `json:"revision"`
	Ready      bool   `json:"ready"`
}

// DefineView creates or replaces a view. A rejected definition returns an
// *InvalidViewError carrying Core's detail.
func (c *Client) DefineView(ctx context.Context, name string, def ViewDefinition) (*View, error) {
	respData, _, err := c.do(ctx, http.MethodPut, viewPath(name), def)
	if err != nil {
		if apiErr, ok := apiErrorWith(err, http.StatusBadRequest, "invalid_view"); ok {
			return nil, &InvalidViewError{Detail: apiErr.Detail, API: apiErr}
		}
		return nil, err
	}
	return decodeView(respData)
}

// GetView reads a view's definition, generation and fold progress.
func (c *Client) GetView(ctx context.Context, name string) (*View, error) {
	respData, _, err := c.do(ctx, http.MethodGet, viewPath(name), nil)
	if err != nil {
		return nil, err
	}
	return decodeView(respData)
}

// DeleteView removes a view's definition and rows.
func (c *Client) DeleteView(ctx context.Context, name string) error {
	_, _, err := c.do(ctx, http.MethodDelete, viewPath(name), nil)
	return err
}

// ListViews returns the tenant's views.
func (c *Client) ListViews(ctx context.Context) ([]ViewSummary, error) {
	respData, _, err := c.do(ctx, http.MethodGet, "/api/v1/views", nil)
	if err != nil {
		return nil, err
	}
	var resp struct {
		Views []ViewSummary `json:"views"`
	}
	if err := json.Unmarshal(respData, &resp); err != nil {
		return nil, fmt.Errorf("decode views: %w", err)
	}
	return resp.Views, nil
}

func decodeView(data []byte) (*View, error) {
	var v View
	if err := json.Unmarshal(data, &v); err != nil {
		return nil, fmt.Errorf("decode view: %w", err)
	}
	return &v, nil
}

// LatestOptions configures Latest.
type LatestOptions struct {
	// IfNoneMatch is an ETag from an earlier ViewRow.
	IfNoneMatch string
}

// ViewRow is the latest state of one view row.
type ViewRow struct {
	View  string         `json:"view"`
	Key   string         `json:"key"`
	State map[string]any `json:"state"`
	// Version is the model revision that last wrote this row.
	Version    int64  `json:"version"`
	Revision   int64  `json:"revision"`
	Generation int64  `json:"generation"`
	Ready      bool   `json:"ready"`
	ETag       string `json:"-"`
}

// Latest reads one view row. A missing row returns a *ViewRowNotFoundError;
// with opts.IfNoneMatch an unchanged row returns a *NotModifiedError.
func (c *Client) Latest(ctx context.Context, view, key string, opts LatestOptions) (*ViewRow, error) {
	var row ViewRow
	etag, err := c.getJSON(ctx, viewPath(view, "entities", key), opts.IfNoneMatch, "view row", &row)
	if err != nil {
		if apiErr, ok := apiErrorWith(err, http.StatusNotFound, ""); ok {
			ready := apiErr.Ready != nil && *apiErr.Ready
			return nil, &ViewRowNotFoundError{View: view, Key: key, Ready: ready, API: apiErr}
		}
		return nil, err
	}
	row.ETag = etag
	return &row, nil
}

// ViewEntity is one row of a view entities page.
type ViewEntity struct {
	Key     string         `json:"key"`
	State   map[string]any `json:"state"`
	Version int64          `json:"version"`
}

// ViewEntitiesOptions configures one ViewEntities page.
type ViewEntitiesOptions struct {
	Prefix string
	// After is the Next value of the previous page.
	After string
	// Limit is 1..500; the server defaults to 100 when zero.
	Limit int
}

// ViewEntitiesPage is one page of view rows in key order.
type ViewEntitiesPage struct {
	View string       `json:"view"`
	Rows []ViewEntity `json:"rows"`
	Next *string      `json:"next"`
	// Revision is the cursor to start ViewChanges from after the last page.
	Revision        int64 `json:"revision"`
	ContentRevision int64 `json:"content_revision"`
	Generation      int64 `json:"generation"`
	Ready           bool  `json:"ready"`
}

func (p *ViewEntitiesPage) last() bool {
	return p.Next == nil || *p.Next == ""
}

// ViewEntities reads one page of view rows.
func (c *Client) ViewEntities(ctx context.Context, view string, opts ViewEntitiesOptions) (*ViewEntitiesPage, error) {
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

	respData, _, err := c.do(ctx, http.MethodGet, withQuery(viewPath(view, "entities"), params), nil)
	if err != nil {
		return nil, err
	}

	var page ViewEntitiesPage
	if err := json.Unmarshal(respData, &page); err != nil {
		return nil, fmt.Errorf("decode view entities: %w", err)
	}
	return &page, nil
}

// LatestManyOptions configures LatestMany and LatestManyPages.
type LatestManyOptions struct {
	Prefix string
	// Limit is the page size, 1..500; the server defaults to 100 when zero.
	Limit int
}

// LatestManyPages calls fn with every page of view rows matching
// opts.Prefix, stopping at the first error fn returns.
func (c *Client) LatestManyPages(ctx context.Context, view string, opts LatestManyOptions, fn func(*ViewEntitiesPage) error) error {
	after := ""
	for {
		page, err := c.ViewEntities(ctx, view, ViewEntitiesOptions{Prefix: opts.Prefix, After: after, Limit: opts.Limit})
		if err != nil {
			return err
		}
		if err := fn(page); err != nil {
			return err
		}
		if page.last() {
			return nil
		}
		after = *page.Next
	}
}

var errStopIteration = errors.New("allsource: iteration stopped")

// LatestMany iterates every view row matching opts.Prefix across all pages.
// A failed page read is yielded once as the error and ends the iteration.
func (c *Client) LatestMany(ctx context.Context, view string, opts LatestManyOptions) iter.Seq2[ViewEntity, error] {
	return func(yield func(ViewEntity, error) bool) {
		err := c.LatestManyPages(ctx, view, opts, func(page *ViewEntitiesPage) error {
			for _, row := range page.Rows {
				if !yield(row, nil) {
					return errStopIteration
				}
			}
			return nil
		})
		if err != nil && !errors.Is(err, errStopIteration) {
			yield(ViewEntity{}, err)
		}
	}
}

// ViewChangesOptions configures one ViewChanges call.
type ViewChangesOptions struct {
	// After is the revision cursor; rows written after it are returned.
	After int64
	Limit int
	// Wait long-polls up to 20s when nothing changed after After.
	Wait time.Duration
}

// ViewChange is one changed row. A nil State is a deletion.
type ViewChange struct {
	Key     string         `json:"key"`
	Version int64          `json:"version"`
	State   map[string]any `json:"state"`
}

// Deleted reports whether the change removed the row.
func (ch ViewChange) Deleted() bool { return ch.State == nil }

// ViewChangesPage is the rows changed after a revision, ordered by version
// then key.
type ViewChangesPage struct {
	View    string       `json:"view"`
	Changes []ViewChange `json:"changes"`
	// Revision is the cursor for the next call.
	Revision   int64 `json:"revision"`
	CaughtUp   bool  `json:"caught_up"`
	Generation int64 `json:"generation"`
}

// ViewChanges reads rows changed after opts.After. A cursor older than the
// view's last reset returns a *ViewResetError.
func (c *Client) ViewChanges(ctx context.Context, view string, opts ViewChangesOptions) (*ViewChangesPage, error) {
	params := url.Values{}
	params.Set("after", strconv.FormatInt(opts.After, 10))
	if opts.Limit > 0 {
		params.Set("limit", strconv.Itoa(opts.Limit))
	}
	if opts.Wait > 0 {
		wait := min(opts.Wait, maxViewWait)
		params.Set("wait", strconv.Itoa(int(math.Ceil(wait.Seconds()))))
	}

	respData, _, err := c.do(ctx, http.MethodGet, withQuery(viewPath(view, "changes"), params), nil)
	if err != nil {
		if apiErr, ok := apiErrorWith(err, http.StatusConflict, "view_reset"); ok {
			var generation int64
			if apiErr.Generation != nil {
				generation = *apiErr.Generation
			}
			return nil, &ViewResetError{View: view, Generation: generation, API: apiErr}
		}
		return nil, err
	}

	var page ViewChangesPage
	if err := json.Unmarshal(respData, &page); err != nil {
		return nil, fmt.Errorf("decode view changes: %w", err)
	}
	return &page, nil
}

// SyncOptions configures Sync.
type SyncOptions struct {
	// Checkpoint resumes from a stored SyncBatch.Checkpoint; nil starts with
	// a full snapshot.
	Checkpoint *int64
	// Wait is the changes long-poll, at most 20s. The HTTP client timeout
	// must exceed it.
	Wait time.Duration
	// PageLimit is the snapshot page size; ChangesLimit caps one changes call.
	PageLimit    int
	ChangesLimit int
	// PollInterval is the minimum time between two caught-up changes polls,
	// default 1s, so a server that answers a long-poll at once is not hammered.
	PollInterval time.Duration
}

// SyncBatch is one unit of work for the Sync callback.
type SyncBatch struct {
	// Reset is set on the first snapshot page: drop every row held so far.
	Reset bool
	// Snapshot is set on batches read from the entity pages.
	Snapshot bool
	// Snapshotting is set on every snapshot page but the last. Checkpoint
	// must not be stored while it is set: a partial snapshot cannot resume.
	Snapshotting bool
	Upserts      []ViewEntity
	Deletes      []string
	// Checkpoint is the cursor to store once the batch is applied.
	Checkpoint int64
	Generation int64
	// Ready is the view's readiness reported by the last snapshot.
	Ready    bool
	CaughtUp bool
}

// Sync keeps a caller's copy of a view current: a snapshot of every entity
// page, then a changes loop. A view reset re-runs the snapshot; a 429 or 503
// carrying Retry-After is waited out and the identical request repeated.
// Sync returns when ctx ends, fn returns an error, or a read fails.
func (c *Client) Sync(ctx context.Context, view string, opts SyncOptions, fn func(SyncBatch) error) error {
	poll := opts.PollInterval
	if poll <= 0 {
		poll = time.Second
	}
	snapshot := opts.Checkpoint == nil
	var cursor int64
	if !snapshot {
		cursor = *opts.Checkpoint
	}
	var ready bool

	for {
		if err := ctx.Err(); err != nil {
			return err
		}
		if snapshot {
			cp, r, err := c.syncSnapshot(ctx, view, opts.PageLimit, fn)
			if err != nil {
				return err
			}
			cursor, ready, snapshot = cp, r, false
		}

		started := time.Now()
		var page *ViewChangesPage
		err := honourRetryAfter(ctx, func() error {
			var err error
			page, err = c.ViewChanges(ctx, view, ViewChangesOptions{After: cursor, Limit: opts.ChangesLimit, Wait: opts.Wait})
			return err
		})
		var reset *ViewResetError
		if errors.As(err, &reset) {
			snapshot = true
			continue
		}
		if err != nil {
			if ctxErr := ctx.Err(); ctxErr != nil {
				return ctxErr
			}
			return err
		}

		if len(page.Changes) > 0 || page.Revision != cursor {
			batch := SyncBatch{Checkpoint: page.Revision, Generation: page.Generation, Ready: ready, CaughtUp: page.CaughtUp}
			for _, ch := range page.Changes {
				if ch.Deleted() {
					batch.Deletes = append(batch.Deletes, ch.Key)
				} else {
					batch.Upserts = append(batch.Upserts, ViewEntity{Key: ch.Key, State: ch.State, Version: ch.Version})
				}
			}
			if err := fn(batch); err != nil {
				return err
			}
			cursor = page.Revision
		}

		if page.CaughtUp {
			if rest := poll - time.Since(started); rest > 0 {
				if err := sleepContext(ctx, rest); err != nil {
					return err
				}
			}
		}
	}
}

// syncSnapshot yields every entity page and returns the changes cursor. The
// cursor is the first page's revision, not the last's: rows committed while
// later pages are read are then replayed by changes instead of skipped.
func (c *Client) syncSnapshot(ctx context.Context, view string, limit int, fn func(SyncBatch) error) (int64, bool, error) {
restart:
	for {
		var watermark, generation int64
		after := ""
		first := true
		for {
			var page *ViewEntitiesPage
			err := honourRetryAfter(ctx, func() error {
				var err error
				page, err = c.ViewEntities(ctx, view, ViewEntitiesOptions{After: after, Limit: limit})
				return err
			})
			if err != nil {
				return 0, false, err
			}
			if first {
				watermark, generation = page.Revision, page.Generation
			} else if page.Generation != generation {
				continue restart
			}

			last := page.last()
			batch := SyncBatch{
				Reset:        first,
				Snapshot:     true,
				Snapshotting: !last,
				Upserts:      page.Rows,
				Checkpoint:   watermark,
				Generation:   generation,
				Ready:        page.Ready,
				CaughtUp:     false,
			}
			if err := fn(batch); err != nil {
				return 0, false, err
			}
			if last {
				return watermark, page.Ready, nil
			}
			after = *page.Next
			first = false
		}
	}
}

func honourRetryAfter(ctx context.Context, call func() error) error {
	for {
		err := call()
		var apiErr *APIError
		if err == nil || !errors.As(err, &apiErr) || !isThrottled(apiErr) || apiErr.RetryAfter <= 0 {
			return err
		}
		if err := sleepContext(ctx, apiErr.RetryAfter); err != nil {
			return err
		}
	}
}

func sleepContext(ctx context.Context, d time.Duration) error {
	timer := time.NewTimer(d)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}

func viewPath(view string, rest ...string) string {
	path := "/api/v1/views/" + url.PathEscape(view)
	for _, segment := range rest {
		path += "/" + url.PathEscape(segment)
	}
	return path
}
