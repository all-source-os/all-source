package allsource

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

const fixtureDir = "../contract/fixtures"

type contractFixture struct {
	Name        string `json:"name"`
	Description string `json:"description"`
	Request     struct {
		Method  string            `json:"method"`
		Path    string            `json:"path"`
		Query   map[string]string `json:"query"`
		Headers map[string]string `json:"headers"`
		Body    json.RawMessage   `json:"body"`
	} `json:"request"`
	Response struct {
		Status  int               `json:"status"`
		Headers map[string]string `json:"headers"`
		Body    json.RawMessage   `json:"body"`
	} `json:"response"`
}

var fixtureRunners = map[string]func(t *testing.T, fx contractFixture){
	"entity_state":                     runEntityState,
	"entity_state_not_modified":        runEntityStateNotModified,
	"event_feed":                       runEventFeed,
	"event_feed_epoch_changed":         runEventFeedEpochChanged,
	"events_query_with_entity_version": runEventsQueryWithEntityVersion,
	"rate_limited_retry_after":         runRateLimitedRetryAfter,
	"read_model_commit":                runReadModelCommit,
	"read_model_commit_conflict":       runReadModelCommitConflict,
	"read_model_page":                  runReadModelPage,
	"read_model_unchanged":             runReadModelUnchanged,
	"view_changes":                     runViewChanges,
	"view_changes_empty_after_wait":    runViewChangesEmptyAfterWait,
	"view_changes_reset":               runViewChangesReset,
	"view_define":                      runViewDefine,
	"view_define_invalid":              runViewDefineInvalid,
	"view_entities_page_1":             runViewEntitiesPage1,
	"view_entities_page_2":             runViewEntitiesPage2,
	"view_get":                         runViewGet,
	"view_latest":                      runViewLatest,
	"view_latest_not_found":            runViewLatestNotFound,
	"view_latest_not_modified":         runViewLatestNotModified,
	"views_list":                       runViewsList,
}

func TestContractFixtures(t *testing.T) {
	paths, err := filepath.Glob(filepath.Join(fixtureDir, "*.json"))
	if err != nil {
		t.Fatalf("glob fixtures: %v", err)
	}
	if len(paths) == 0 {
		t.Fatalf("no fixtures found in %s", fixtureDir)
	}
	for _, path := range paths {
		name := strings.TrimSuffix(filepath.Base(path), ".json")
		fx := loadFixtureFile(t, path)
		if fx.Name != name {
			t.Errorf("fixture %s: name field is %q", path, fx.Name)
		}
		run, ok := fixtureRunners[name]
		if !ok {
			t.Errorf("fixture %s has no Go SDK test", name)
			continue
		}
		t.Run(name, func(t *testing.T) { run(t, fx) })
	}
}

func loadFixtureFile(t *testing.T, path string) contractFixture {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read fixture %s: %v", path, err)
	}
	var fx contractFixture
	if err := json.Unmarshal(data, &fx); err != nil {
		t.Fatalf("decode fixture %s: %v", path, err)
	}
	return fx
}

func loadFixture(t *testing.T, name string) contractFixture {
	t.Helper()
	return loadFixtureFile(t, filepath.Join(fixtureDir, name+".json"))
}

func isJSONNull(raw json.RawMessage) bool {
	trimmed := bytes.TrimSpace(raw)
	return len(trimmed) == 0 || string(trimmed) == "null"
}

func decodedSegments(path string) []string {
	parts := strings.Split(path, "/")
	for i, part := range parts {
		if decoded, err := url.PathUnescape(part); err == nil {
			parts[i] = decoded
		}
	}
	return parts
}

// assertFixtureRequest checks r against fx.Request. allowExtraQuery admits
// parameters the fixture does not list, for multi-step scenarios such as sync
// sending wait on every changes call.
func assertFixtureRequest(t *testing.T, fx contractFixture, r *http.Request, allowExtraQuery bool) {
	t.Helper()
	if r.Method != fx.Request.Method {
		t.Errorf("%s: method %s, want %s", fx.Name, r.Method, fx.Request.Method)
	}
	if got, want := decodedSegments(r.URL.EscapedPath()), decodedSegments(fx.Request.Path); !reflect.DeepEqual(got, want) {
		t.Errorf("%s: path %s, want %s", fx.Name, r.URL.EscapedPath(), fx.Request.Path)
	}
	query := r.URL.Query()
	for key, want := range fx.Request.Query {
		if got := query.Get(key); got != want {
			t.Errorf("%s: query %s=%q, want %q", fx.Name, key, got, want)
		}
	}
	if !allowExtraQuery {
		for key := range query {
			if _, ok := fx.Request.Query[key]; !ok {
				t.Errorf("%s: unexpected query parameter %s=%q", fx.Name, key, query.Get(key))
			}
		}
	}
	for name, want := range fx.Request.Headers {
		if got := r.Header.Get(name); got != want {
			t.Errorf("%s: header %s=%q, want %q", fx.Name, name, got, want)
		}
	}

	body, err := io.ReadAll(r.Body)
	if err != nil {
		t.Errorf("%s: read request body: %v", fx.Name, err)
		return
	}
	if isJSONNull(fx.Request.Body) {
		if len(bytes.TrimSpace(body)) != 0 {
			t.Errorf("%s: unexpected request body %s", fx.Name, body)
		}
		return
	}
	var got, want any
	if err := json.Unmarshal(body, &got); err != nil {
		t.Errorf("%s: request body is not JSON: %v (%s)", fx.Name, err, body)
		return
	}
	if err := json.Unmarshal(fx.Request.Body, &want); err != nil {
		t.Fatalf("%s: fixture body: %v", fx.Name, err)
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("%s: request body\n got %s\nwant %s", fx.Name, body, fx.Request.Body)
	}
}

func writeFixtureResponse(w http.ResponseWriter, fx contractFixture) {
	for name, value := range fx.Response.Headers {
		w.Header().Set(name, value)
	}
	if isJSONNull(fx.Response.Body) {
		w.WriteHeader(fx.Response.Status)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(fx.Response.Status)
	w.Write(fx.Response.Body)
}

func fixtureClient(t *testing.T, fx contractFixture, opts ...Option) (*Client, *atomic.Int32) {
	t.Helper()
	var hits atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits.Add(1)
		assertFixtureRequest(t, fx, r, false)
		writeFixtureResponse(w, fx)
	}))
	t.Cleanup(srv.Close)
	return New("test-api-key", srv.URL, opts...), &hits
}

func fastRetry() Option {
	return WithRetry(RetryConfig{MaxRetries: 2, BaseDelay: time.Millisecond, BackoffFactor: 1, MaxDelay: 5 * time.Millisecond})
}

func runEntityState(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	st, err := c.EntityState(context.Background(), "skill:abc", EntityStateOptions{})
	if err != nil {
		t.Fatalf("EntityState: %v", err)
	}
	if st.EntityID != "skill:abc" || st.Version != 12 || st.EventCount != 12 {
		t.Errorf("unexpected state %+v", st)
	}
	if st.CurrentState["status"] != "published" || st.CurrentState["name"] != "x" {
		t.Errorf("unexpected current_state %v", st.CurrentState)
	}
	if st.AsOf != nil {
		t.Errorf("expected as_of nil, got %v", *st.AsOf)
	}
	if st.LastUpdated == nil || *st.LastUpdated != "2026-10-27T10:00:00Z" {
		t.Errorf("unexpected last_updated %v", st.LastUpdated)
	}
	if st.History != nil {
		t.Errorf("expected no history, got %v", st.History)
	}
	if st.ETag != fx.Response.Headers["ETag"] {
		t.Errorf("ETag %q, want %q", st.ETag, fx.Response.Headers["ETag"])
	}
}

func assertNotModified(t *testing.T, err error, etag string) {
	t.Helper()
	if !errors.Is(err, ErrNotModified) {
		t.Fatalf("expected ErrNotModified, got %v", err)
	}
	var nm *NotModifiedError
	if !errors.As(err, &nm) {
		t.Fatalf("expected *NotModifiedError, got %T", err)
	}
	if nm.ETag != etag {
		t.Errorf("NotModified ETag %q, want %q", nm.ETag, etag)
	}
}

func runEntityStateNotModified(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	st, err := c.EntityState(context.Background(), "skill:abc", EntityStateOptions{IfNoneMatch: fx.Request.Headers["If-None-Match"]})
	if st != nil {
		t.Errorf("expected no state on 304, got %+v", st)
	}
	assertNotModified(t, err, fx.Response.Headers["ETag"])
}

func runEventFeed(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	page, err := c.Feed(context.Background(), FeedOptions{
		Cursor:   &FeedCursor{Epoch: 41, Position: 0},
		Prefixes: []string{"skill."},
		Limit:    100,
	})
	if err != nil {
		t.Fatalf("Feed: %v", err)
	}
	if page.TenantID != "t1" || !page.CaughtUp {
		t.Errorf("unexpected page %+v", page)
	}
	if page.Head != (FeedCursor{Epoch: 41, Position: 3}) {
		t.Errorf("unexpected head %+v", page.Head)
	}
	if len(page.Events) != 1 || page.Events[0].EventType != "skill.published" || page.Events[0].EntityID != "skill:abc" {
		t.Errorf("unexpected events %+v", page.Events)
	}
	if next := page.Next(); next != (FeedCursor{Epoch: 41, Position: 1}) {
		t.Errorf("Next() = %+v, want {41 1}", next)
	}
}

func runEventFeedEpochChanged(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	_, err := c.Feed(context.Background(), FeedOptions{Cursor: &FeedCursor{Epoch: 40, Position: 17}})
	var epochErr *EpochChangedError
	if !errors.As(err, &epochErr) {
		t.Fatalf("expected *EpochChangedError, got %T: %v", err, err)
	}
	if epochErr.ExpectedEpoch != 40 || epochErr.CurrentEpoch != 41 {
		t.Errorf("unexpected epochs %+v", epochErr)
	}
	var apiErr *APIError
	if !errors.As(err, &apiErr) || apiErr.StatusCode != http.StatusConflict {
		t.Errorf("expected wrapped 409 *APIError, got %v", err)
	}
}

func runEventsQueryWithEntityVersion(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	list, err := c.Query(context.Background(), QueryOptions{EntityID: "skill:abc", Limit: 1})
	if err != nil {
		t.Fatalf("Query: %v", err)
	}
	if list.EntityVersion == nil || *list.EntityVersion != 12 {
		t.Errorf("unexpected entity_version %v", list.EntityVersion)
	}
	if list.Count != 1 || list.TotalCount != 12 || !list.HasMore {
		t.Errorf("unexpected list %+v", list)
	}
	if list.ETag != fx.Response.Headers["ETag"] {
		t.Errorf("ETag %q, want %q", list.ETag, fx.Response.Headers["ETag"])
	}
}

func runRateLimitedRetryAfter(t *testing.T, fx contractFixture) {
	c, hits := fixtureClient(t, fx)
	_, err := c.ViewChanges(context.Background(), "ops_status", ViewChangesOptions{After: 61})
	var apiErr *APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("expected *APIError, got %T: %v", err, err)
	}
	if !apiErr.IsRateLimited() {
		t.Errorf("expected 429, got %d", apiErr.StatusCode)
	}
	if apiErr.RetryAfter != time.Second {
		t.Errorf("RetryAfter %s, want 1s", apiErr.RetryAfter)
	}
	if hits.Load() != 1 {
		t.Errorf("expected 1 request without a retry config, got %d", hits.Load())
	}
}

func runReadModelCommit(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	rev, err := c.ReadModel("skills").Commit(context.Background(), ReadModelCommit{
		ExpectedRevision: 7,
		Checkpoint:       FeedCursor{Epoch: 41, Position: 3},
		Ready:            true,
		Changes: map[string]any{
			"skill:abc": map[string]any{"status": "retired"},
			"skill:old": nil,
		},
	})
	if err != nil {
		t.Fatalf("Commit: %v", err)
	}
	if rev != 8 {
		t.Errorf("revision %d, want 8", rev)
	}
}

func runReadModelCommitConflict(t *testing.T, fx contractFixture) {
	c, hits := fixtureClient(t, fx, fastRetry())
	_, err := c.ReadModel("skills").Commit(context.Background(), ReadModelCommit{
		ExpectedRevision: 6,
		Checkpoint:       FeedCursor{Epoch: 41, Position: 3},
		Ready:            true,
		Changes:          map[string]any{"skill:abc": map[string]any{"status": "retired"}},
	})
	var conflict *VersionConflictError
	if !errors.As(err, &conflict) {
		t.Fatalf("expected *VersionConflictError, got %T: %v", err, err)
	}
	if conflict.ExpectedRevision != 6 || conflict.CurrentRevision != 7 {
		t.Errorf("unexpected conflict %+v", conflict)
	}
	if hits.Load() != 1 {
		t.Errorf("a conflicting commit must not be retried, got %d requests", hits.Load())
	}
}

func runReadModelPage(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	page, err := c.ReadModel("skills").Page(context.Background(), ReadModelPageOptions{Prefix: "skill:", Limit: 100})
	if err != nil {
		t.Fatalf("Page: %v", err)
	}
	if page.Unchanged || !page.Ready || page.ContentRevision != 7 || page.Revision != 7 || page.Next != nil {
		t.Errorf("unexpected page %+v", page)
	}
	if page.Checkpoint == nil || *page.Checkpoint != (FeedCursor{Epoch: 41, Position: 3}) {
		t.Errorf("unexpected checkpoint %v", page.Checkpoint)
	}
	row, ok := page.Rows["skill:abc"].(map[string]any)
	if !ok || row["status"] != "published" {
		t.Errorf("unexpected rows %v", page.Rows)
	}
}

func runReadModelUnchanged(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	rev := int64(7)
	page, err := c.ReadModel("skills").Page(context.Background(), ReadModelPageOptions{Prefix: "skill:", Revision: &rev})
	if err != nil {
		t.Fatalf("Page: %v", err)
	}
	if !page.Unchanged {
		t.Error("expected Unchanged")
	}
	if len(page.Rows) != 0 {
		t.Errorf("expected no rows, got %v", page.Rows)
	}
}

func runViewChanges(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	page, err := c.ViewChanges(context.Background(), "ops_status", ViewChangesOptions{After: 61})
	if err != nil {
		t.Fatalf("ViewChanges: %v", err)
	}
	if page.Revision != 62 || !page.CaughtUp || page.Generation != 2 || len(page.Changes) != 2 {
		t.Fatalf("unexpected page %+v", page)
	}
	upsert, deletion := page.Changes[0], page.Changes[1]
	if upsert.Key != "probe-a:2026-10-27" || upsert.Deleted() || upsert.State["ok"] != float64(1431) {
		t.Errorf("unexpected upsert %+v", upsert)
	}
	if deletion.Key != "probe-z:2026-10-20" || !deletion.Deleted() || deletion.Version != 62 {
		t.Errorf("unexpected deletion %+v", deletion)
	}
}

func runViewChangesEmptyAfterWait(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	page, err := c.ViewChanges(context.Background(), "ops_status", ViewChangesOptions{After: 62, Wait: 20 * time.Second})
	if err != nil {
		t.Fatalf("ViewChanges: %v", err)
	}
	if len(page.Changes) != 0 || !page.CaughtUp || page.Revision != 62 {
		t.Errorf("unexpected page %+v", page)
	}
}

func runViewChangesReset(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	_, err := c.ViewChanges(context.Background(), "ops_status", ViewChangesOptions{After: 12})
	var reset *ViewResetError
	if !errors.As(err, &reset) {
		t.Fatalf("expected *ViewResetError, got %T: %v", err, err)
	}
	if reset.Generation != 3 || reset.View != "ops_status" {
		t.Errorf("unexpected reset %+v", reset)
	}
}

func fixtureDefinition(t *testing.T, fx contractFixture) ViewDefinition {
	t.Helper()
	var def ViewDefinition
	if err := json.Unmarshal(fx.Request.Body, &def); err != nil {
		t.Fatalf("decode fixture definition: %v", err)
	}
	return def
}

func runViewDefine(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	view, err := c.DefineView(context.Background(), "ops_status", fixtureDefinition(t, fx))
	if err != nil {
		t.Fatalf("DefineView: %v", err)
	}
	if view.View != "ops_status" || view.Generation != 1 || view.Revision != 0 || view.Ready || view.Checkpoint != nil {
		t.Errorf("unexpected view %+v", view)
	}
	if view.Definition == nil || len(view.Definition.Ops) != 5 || view.Definition.Ops[1].Increment["ok"] != 1 {
		t.Errorf("unexpected definition %+v", view.Definition)
	}
}

func runViewDefineInvalid(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	_, err := c.DefineView(context.Background(), "ops_status", fixtureDefinition(t, fx))
	var invalid *InvalidViewError
	if !errors.As(err, &invalid) {
		t.Fatalf("expected *InvalidViewError, got %T: %v", err, err)
	}
	if invalid.Detail != "ops must contain 1..64 operations" {
		t.Errorf("unexpected detail %q", invalid.Detail)
	}
}

func runViewEntitiesPage1(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	page, err := c.ViewEntities(context.Background(), "ops_status", ViewEntitiesOptions{Limit: 2})
	if err != nil {
		t.Fatalf("ViewEntities: %v", err)
	}
	if len(page.Rows) != 2 || page.Rows[0].Key != "probe-a:2026-10-26" || page.Rows[0].Version != 40 {
		t.Errorf("unexpected rows %+v", page.Rows)
	}
	if page.Next == nil || *page.Next != "probe-a:2026-10-27" {
		t.Errorf("unexpected next %v", page.Next)
	}
	if page.Revision != 61 || page.Generation != 2 || !page.Ready {
		t.Errorf("unexpected page %+v", page)
	}
}

func runViewEntitiesPage2(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	page, err := c.ViewEntities(context.Background(), "ops_status", ViewEntitiesOptions{After: "probe-a:2026-10-27", Limit: 2})
	if err != nil {
		t.Fatalf("ViewEntities: %v", err)
	}
	if len(page.Rows) != 1 || page.Rows[0].Key != "probe-z:2026-10-20" {
		t.Errorf("unexpected rows %+v", page.Rows)
	}
	if page.Next != nil || page.Revision != 61 {
		t.Errorf("unexpected page %+v", page)
	}
}

func runViewGet(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	view, err := c.GetView(context.Background(), "ops_status")
	if err != nil {
		t.Fatalf("GetView: %v", err)
	}
	if view.Generation != 2 || view.Revision != 62 || !view.Ready {
		t.Errorf("unexpected view %+v", view)
	}
	if view.Checkpoint == nil || *view.Checkpoint != (FeedCursor{Epoch: 41, Position: 9120}) {
		t.Errorf("unexpected checkpoint %v", view.Checkpoint)
	}
	if view.Definition == nil || view.Definition.Key != "{payload.probe}:{day:timestamp}" {
		t.Errorf("unexpected definition %+v", view.Definition)
	}
}

func runViewLatest(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	row, err := c.Latest(context.Background(), "ops_status", "probe-a:2026-10-27", LatestOptions{})
	if err != nil {
		t.Fatalf("Latest: %v", err)
	}
	if row.Key != "probe-a:2026-10-27" || row.Version != 57 || row.Revision != 61 || row.Generation != 2 || !row.Ready {
		t.Errorf("unexpected row %+v", row)
	}
	if row.State["ok"] != float64(1430) {
		t.Errorf("unexpected state %v", row.State)
	}
	if row.ETag != fx.Response.Headers["ETag"] {
		t.Errorf("ETag %q, want %q", row.ETag, fx.Response.Headers["ETag"])
	}
}

func runViewLatestNotFound(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	_, err := c.Latest(context.Background(), "ops_status", "probe-q:2026-10-27", LatestOptions{})
	var notFound *ViewRowNotFoundError
	if !errors.As(err, &notFound) {
		t.Fatalf("expected *ViewRowNotFoundError, got %T: %v", err, err)
	}
	if !notFound.Ready || notFound.Key != "probe-q:2026-10-27" {
		t.Errorf("unexpected not-found %+v", notFound)
	}
	var apiErr *APIError
	if !errors.As(err, &apiErr) || !apiErr.IsNotFound() {
		t.Errorf("expected wrapped 404 *APIError, got %v", err)
	}
}

func runViewLatestNotModified(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	row, err := c.Latest(context.Background(), "ops_status", "probe-a:2026-10-27", LatestOptions{IfNoneMatch: fx.Request.Headers["If-None-Match"]})
	if row != nil {
		t.Errorf("expected no row on 304, got %+v", row)
	}
	assertNotModified(t, err, fx.Response.Headers["ETag"])
}

func runViewsList(t *testing.T, fx contractFixture) {
	c, _ := fixtureClient(t, fx)
	views, err := c.ListViews(context.Background())
	if err != nil {
		t.Fatalf("ListViews: %v", err)
	}
	want := []ViewSummary{
		{View: "ops_status", Generation: 2, Revision: 62, Ready: true},
		{View: "skill_catalog", Generation: 1, Revision: 0, Ready: false},
	}
	if !reflect.DeepEqual(views, want) {
		t.Errorf("ListViews = %+v, want %+v", views, want)
	}
}

// TestSyncScenarioFromFixtures plays the README's sync scenario: a stale
// checkpoint is reset, the snapshot is re-read, a 429 is waited out, changes
// are applied, and an empty long-poll yields nothing.
func TestSyncScenarioFromFixtures(t *testing.T) {
	script := []contractFixture{
		loadFixture(t, "view_changes_reset"),
		loadFixture(t, "view_entities_page_1"),
		loadFixture(t, "view_entities_page_2"),
		loadFixture(t, "rate_limited_retry_after"),
		loadFixture(t, "view_changes"),
		loadFixture(t, "view_changes_empty_after_wait"),
	}
	idle := script[len(script)-1]

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	var mu sync.Mutex
	step := 0
	var throttledAt, retriedAt time.Time
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		i := step
		step++
		switch i {
		case 3:
			throttledAt = time.Now()
		case 4:
			retriedAt = time.Now()
		}
		mu.Unlock()
		if i >= len(script) {
			cancel()
			assertFixtureRequest(t, idle, r, true)
			writeFixtureResponse(w, idle)
			return
		}
		assertFixtureRequest(t, script[i], r, true)
		writeFixtureResponse(w, script[i])
	}))
	defer srv.Close()

	c := New("test-api-key", srv.URL)
	stale := int64(12)
	state := map[string]ViewEntity{}
	var stored int64
	var batches []SyncBatch
	err := c.Sync(ctx, "ops_status", SyncOptions{
		Checkpoint:   &stale,
		Wait:         20 * time.Second,
		PageLimit:    2,
		PollInterval: time.Millisecond,
	}, func(b SyncBatch) error {
		batches = append(batches, b)
		if b.Reset {
			clear(state)
		}
		for _, row := range b.Upserts {
			state[row.Key] = row
		}
		for _, key := range b.Deletes {
			delete(state, key)
		}
		if !b.Snapshotting {
			stored = b.Checkpoint
		}
		return nil
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Sync returned %v, want context.Canceled", err)
	}

	if len(batches) != 3 {
		t.Fatalf("expected 2 snapshot batches and 1 change batch, got %d: %+v", len(batches), batches)
	}
	if !batches[0].Reset || !batches[0].Snapshotting || !batches[0].Snapshot {
		t.Errorf("first batch must reset and be an incomplete snapshot: %+v", batches[0])
	}
	if batches[1].Snapshotting || batches[1].Checkpoint != 61 {
		t.Errorf("second batch must complete the snapshot at 61: %+v", batches[1])
	}
	if batches[2].Snapshot || batches[2].Checkpoint != 62 {
		t.Errorf("third batch must be changes at 62: %+v", batches[2])
	}
	if stored != 62 {
		t.Errorf("stored checkpoint %d, want 62", stored)
	}
	mu.Lock()
	waited := retriedAt.Sub(throttledAt)
	mu.Unlock()
	if waited < time.Second {
		t.Errorf("retried %s after a Retry-After: 1, want at least 1s", waited)
	}

	if len(state) != 2 {
		t.Fatalf("expected 2 rows, got %v", state)
	}
	if row := state["probe-a:2026-10-26"]; row.Version != 40 {
		t.Errorf("probe-a:2026-10-26 = %+v, want version 40", row)
	}
	if row := state["probe-a:2026-10-27"]; row.Version != 62 || row.State["ok"] != float64(1431) {
		t.Errorf("probe-a:2026-10-27 = %+v, want version 62 ok 1431", row)
	}
	if _, ok := state["probe-z:2026-10-20"]; ok {
		t.Error("probe-z:2026-10-20 must be deleted")
	}
}
