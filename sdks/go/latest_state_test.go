package allsource

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"sync/atomic"
	"testing"
	"time"
)

func TestEntityStateSendsHistoryAndAsOf(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query()
		if q.Get("history") != "true" || q.Get("as_of") != "2026-10-01T00:00:00Z" {
			t.Errorf("unexpected query %s", r.URL.RawQuery)
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"entity_id": "skill:abc", "current_state": map[string]any{}, "event_count": 2,
			"version": 2, "history": []any{map[string]any{"event_type": "skill.created"}},
		})
	})
	defer srv.Close()

	st, err := client.EntityState(context.Background(), "skill:abc", EntityStateOptions{History: true, AsOf: "2026-10-01T00:00:00Z"})
	if err != nil {
		t.Fatalf("EntityState: %v", err)
	}
	if len(st.History) != 1 || st.History[0]["event_type"] != "skill.created" {
		t.Errorf("unexpected history %v", st.History)
	}
}

func TestEntityStateEscapesSlashInID(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.EscapedPath() != "/api/v1/entities/a%2Fb/state" {
			t.Errorf("unexpected path %s", r.URL.EscapedPath())
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"entity_id":"a/b","current_state":{},"event_count":0,"version":0}`))
	})
	defer srv.Close()

	if _, err := client.EntityState(context.Background(), "a/b", EntityStateOptions{}); err != nil {
		t.Fatalf("EntityState: %v", err)
	}
}

func TestQueryIfNoneMatchReturnsNotModified(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("If-None-Match") != `"sha256:aa"` {
			t.Errorf("unexpected If-None-Match %q", r.Header.Get("If-None-Match"))
		}
		w.Header().Set("ETag", `"sha256:aa"`)
		w.WriteHeader(http.StatusNotModified)
	})
	defer srv.Close()

	list, err := client.Query(context.Background(), QueryOptions{EntityID: "x", IfNoneMatch: `"sha256:aa"`})
	if list != nil {
		t.Errorf("expected no list, got %+v", list)
	}
	assertNotModified(t, err, `"sha256:aa"`)
}

func TestNotModifiedIsNotACircuitBreakerFailure(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotModified)
	}))
	defer srv.Close()

	client := New("key", srv.URL, WithCircuitBreaker(1, time.Minute))
	for i := 0; i < 3; i++ {
		_, err := client.Latest(context.Background(), "v", "k", LatestOptions{IfNoneMatch: `"t"`})
		if !errors.Is(err, ErrNotModified) {
			t.Fatalf("attempt %d: expected ErrNotModified, got %v", i, err)
		}
	}
}

func TestFeedSendsEntityIDAndJoinsPrefixes(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query()
		if q.Get("entity_id") != "skill:abc" || q.Get("prefixes") != "skill.,ops." {
			t.Errorf("unexpected query %s", r.URL.RawQuery)
		}
		if q.Has("epoch") || q.Has("position") || q.Has("limit") {
			t.Errorf("cursor and limit must be omitted: %s", r.URL.RawQuery)
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"tenant_id":"t1","cursor":{"epoch":3,"position":9},"head":{"epoch":3,"position":9},"events":[],"positions":[],"caught_up":true}`))
	})
	defer srv.Close()

	page, err := client.Feed(context.Background(), FeedOptions{EntityID: "skill:abc", Prefixes: []string{"skill.", "ops."}})
	if err != nil {
		t.Fatalf("Feed: %v", err)
	}
	if page.Next() != (FeedCursor{Epoch: 3, Position: 9}) {
		t.Errorf("an empty page must keep its cursor, got %+v", page.Next())
	}
}

func TestReadModelCommitSendsEmptyChangesObject(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		var body map[string]any
		json.NewDecoder(r.Body).Decode(&body)
		if changes, ok := body["changes"].(map[string]any); !ok || len(changes) != 0 {
			t.Errorf("expected changes {}, got %v", body["changes"])
		}
		if body["reset"] != true {
			t.Errorf("expected reset true, got %v", body["reset"])
		}
		w.Write([]byte(`{"revision":1}`))
	})
	defer srv.Close()

	rev, err := client.ReadModel("m").Commit(context.Background(), ReadModelCommit{Reset: true})
	if err != nil || rev != 1 {
		t.Fatalf("Commit = %d, %v", rev, err)
	}
}

func TestDeleteView(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodDelete || r.URL.Path != "/api/v1/views/ops_status" {
			t.Errorf("unexpected %s %s", r.Method, r.URL.Path)
		}
		w.WriteHeader(http.StatusNoContent)
	})
	defer srv.Close()

	if err := client.DeleteView(context.Background(), "ops_status"); err != nil {
		t.Fatalf("DeleteView: %v", err)
	}
}

func TestDefineViewOmitsUnsetOperations(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		var body struct {
			Ops []map[string]any `json:"ops"`
		}
		json.NewDecoder(r.Body).Decode(&body)
		if len(body.Ops) != 2 {
			t.Errorf("expected 2 ops, got %v", body.Ops)
			return
		}
		if len(body.Ops[0]) != 2 || body.Ops[0]["delete"] != true {
			t.Errorf("expected only on and delete, got %v", body.Ops[0])
		}
		status, _ := body.Ops[1]["status"].(map[string]any)
		if status["from"] != "suffix" || status["field"] != "state" {
			t.Errorf("unexpected status op %v", body.Ops[1])
		}
		set, _ := body.Ops[1]["set"].(map[string]any)
		lit, _ := set["kind"].(map[string]any)
		if lit["literal"] != "probe" {
			t.Errorf("expected literal expression, got %v", body.Ops[1]["set"])
		}
		w.Write([]byte(`{"view":"v","definition":null,"generation":1,"revision":0,"checkpoint":null,"ready":false}`))
	})
	defer srv.Close()

	_, err := client.DefineView(context.Background(), "v", ViewDefinition{
		Source: ViewSource{Prefixes: []string{"ops."}},
		Key:    "{entity_id}",
		Ops: []ViewOp{
			{On: "ops.removed", Delete: true},
			{On: "ops.*", Status: &ViewStatusOp{Field: "state", From: "suffix"}, Set: map[string]any{"kind": ViewLiteral("probe")}},
		},
	})
	if err != nil {
		t.Fatalf("DefineView: %v", err)
	}
}

func viewPagesServer(t *testing.T, pages map[string]string) (*httptest.Server, *Client, *atomic.Int32) {
	t.Helper()
	var hits atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits.Add(1)
		body, ok := pages[r.URL.Query().Get("after")]
		if !ok {
			t.Errorf("unexpected after %q", r.URL.Query().Get("after"))
			w.WriteHeader(http.StatusBadRequest)
			return
		}
		if r.URL.Query().Get("prefix") != "p:" {
			t.Errorf("prefix not sent on every page: %s", r.URL.RawQuery)
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(body))
	}))
	t.Cleanup(srv.Close)
	return srv, New("key", srv.URL), &hits
}

var twoViewPages = map[string]string{
	"":    `{"view":"v","rows":[{"key":"p:1","state":{},"version":1},{"key":"p:2","state":{},"version":2}],"next":"p:2","revision":5,"content_revision":5,"generation":1,"ready":true}`,
	"p:2": `{"view":"v","rows":[{"key":"p:3","state":{},"version":3}],"next":null,"revision":5,"content_revision":5,"generation":1,"ready":true}`,
}

func TestLatestManyIteratesEveryPage(t *testing.T) {
	_, client, _ := viewPagesServer(t, twoViewPages)
	var keys []string
	for row, err := range client.LatestMany(context.Background(), "v", LatestManyOptions{Prefix: "p:", Limit: 2}) {
		if err != nil {
			t.Fatalf("LatestMany: %v", err)
		}
		keys = append(keys, row.Key)
	}
	if len(keys) != 3 || keys[2] != "p:3" {
		t.Errorf("unexpected keys %v", keys)
	}
}

func TestLatestManyStopsFetchingWhenTheLoopBreaks(t *testing.T) {
	_, client, hits := viewPagesServer(t, twoViewPages)
	for row, err := range client.LatestMany(context.Background(), "v", LatestManyOptions{Prefix: "p:"}) {
		if err != nil {
			t.Fatalf("LatestMany: %v", err)
		}
		if row.Key == "p:1" {
			break
		}
	}
	if hits.Load() != 1 {
		t.Errorf("expected 1 page read, got %d", hits.Load())
	}
}

func TestLatestManyYieldsAPageError(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusForbidden)
		w.Write([]byte(`{"error":"forbidden"}`))
	})
	defer srv.Close()

	var got error
	for _, err := range client.LatestMany(context.Background(), "v", LatestManyOptions{}) {
		got = err
	}
	var apiErr *APIError
	if !errors.As(got, &apiErr) || !apiErr.IsForbidden() || apiErr.Code != "forbidden" {
		t.Errorf("expected 403 forbidden, got %v", got)
	}
}

func TestLatestManyPagesStopsOnCallbackError(t *testing.T) {
	_, client, hits := viewPagesServer(t, twoViewPages)
	stop := errors.New("stop")
	err := client.LatestManyPages(context.Background(), "v", LatestManyOptions{Prefix: "p:"}, func(*ViewEntitiesPage) error { return stop })
	if !errors.Is(err, stop) || hits.Load() != 1 {
		t.Errorf("expected callback error after 1 page, got %v after %d", err, hits.Load())
	}
}

func TestViewChangesClampsWaitToTwentySeconds(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Query().Get("wait") != "20" || r.URL.Query().Get("after") != "0" {
			t.Errorf("unexpected query %s", r.URL.RawQuery)
		}
		w.Write([]byte(`{"view":"v","changes":[],"revision":0,"caught_up":true,"generation":1}`))
	})
	defer srv.Close()

	if _, err := client.ViewChanges(context.Background(), "v", ViewChangesOptions{Wait: time.Minute}); err != nil {
		t.Fatalf("ViewChanges: %v", err)
	}
}

func TestSyncWithoutCheckpointSnapshotsFirst(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	var polled atomic.Bool
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/api/v1/views/v/entities":
			w.Write([]byte(`{"view":"v","rows":[{"key":"a","state":{"n":1},"version":4}],"next":null,"revision":4,"content_revision":4,"generation":1,"ready":true}`))
		case "/api/v1/views/v/changes":
			polled.Store(true)
			if r.URL.Query().Get("after") != "4" {
				t.Errorf("changes must start at the snapshot revision, got %s", r.URL.RawQuery)
			}
			cancel()
			w.Write([]byte(`{"view":"v","changes":[],"revision":4,"caught_up":true,"generation":1}`))
		default:
			t.Errorf("unexpected path %s", r.URL.Path)
		}
	})
	defer srv.Close()

	var batches []SyncBatch
	err := client.Sync(ctx, "v", SyncOptions{PollInterval: time.Millisecond}, func(b SyncBatch) error {
		batches = append(batches, b)
		return nil
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Sync returned %v", err)
	}
	if len(batches) != 1 || !batches[0].Reset || batches[0].Snapshotting || batches[0].Checkpoint != 4 || !batches[0].Ready {
		t.Errorf("unexpected batches %+v", batches)
	}
	if !polled.Load() {
		t.Error("changes were never polled")
	}
}

func TestSyncRestartsSnapshotWhenGenerationChangesBetweenPages(t *testing.T) {
	var entityCalls atomic.Int32
	stop := errors.New("stop")
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		switch entityCalls.Add(1) {
		case 1:
			w.Write([]byte(`{"view":"v","rows":[{"key":"a","state":{},"version":1}],"next":"a","revision":3,"content_revision":3,"generation":1,"ready":true}`))
		case 2:
			w.Write([]byte(`{"view":"v","rows":[{"key":"b","state":{},"version":9}],"next":null,"revision":9,"content_revision":9,"generation":2,"ready":true}`))
		case 3:
			if r.URL.Query().Has("after") {
				t.Errorf("a restarted snapshot must begin at the first page: %s", r.URL.RawQuery)
			}
			w.Write([]byte(`{"view":"v","rows":[{"key":"b","state":{},"version":9}],"next":null,"revision":9,"content_revision":9,"generation":2,"ready":true}`))
		default:
			t.Errorf("unexpected request %s", r.URL)
		}
	})
	defer srv.Close()

	var batches []SyncBatch
	err := client.Sync(context.Background(), "v", SyncOptions{}, func(b SyncBatch) error {
		batches = append(batches, b)
		if !b.Snapshotting {
			return stop
		}
		return nil
	})
	if !errors.Is(err, stop) {
		t.Fatalf("Sync returned %v", err)
	}
	if len(batches) != 2 || !batches[1].Reset || batches[1].Generation != 2 || batches[1].Checkpoint != 9 {
		t.Errorf("unexpected batches %+v", batches)
	}
}

func TestSyncSkipsAnEmptyCaughtUpPoll(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	var polls atomic.Int32
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if polls.Add(1) >= 3 {
			cancel()
		}
		w.Write([]byte(`{"view":"v","changes":[],"revision":7,"caught_up":true,"generation":1}`))
	})
	defer srv.Close()

	from := int64(7)
	called := false
	err := client.Sync(ctx, "v", SyncOptions{Checkpoint: &from, PollInterval: time.Millisecond}, func(SyncBatch) error {
		called = true
		return nil
	})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("Sync returned %v", err)
	}
	if called {
		t.Error("an empty poll at an unchanged revision must not call fn")
	}
}

func TestSyncReturnsAThrottleWithoutRetryAfter(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusServiceUnavailable)
	})
	defer srv.Close()

	from := int64(1)
	err := client.Sync(context.Background(), "v", SyncOptions{Checkpoint: &from}, func(SyncBatch) error { return nil })
	var apiErr *APIError
	if !errors.As(err, &apiErr) || apiErr.StatusCode != http.StatusServiceUnavailable {
		t.Errorf("expected 503, got %v", err)
	}
}

func TestRetryWaitsForRetryAfter(t *testing.T) {
	var attempts atomic.Int32
	var first, waited atomic.Int64
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if attempts.Add(1) == 1 {
			first.Store(time.Now().UnixNano())
			w.Header().Set("Retry-After", "1")
			w.WriteHeader(http.StatusTooManyRequests)
			return
		}
		waited.Store(time.Now().UnixNano() - first.Load())
		w.Write([]byte(`{"status":"healthy"}`))
	}))
	defer srv.Close()

	client := New("key", srv.URL, WithRetry(RetryConfig{MaxRetries: 1, BaseDelay: time.Millisecond, BackoffFactor: 1, MaxDelay: 10 * time.Millisecond}))
	if _, err := client.Health(context.Background()); err != nil {
		t.Fatalf("Health: %v", err)
	}
	if time.Duration(waited.Load()) < time.Second {
		t.Errorf("retried after %s, want at least the 1s Retry-After", time.Duration(waited.Load()))
	}
}

func TestParseAPIErrorReadsCoreStringCode(t *testing.T) {
	apiErr := parseAPIError([]byte(`{"error":"version_conflict","expected_version":6,"current_version":7}`), 409)
	if apiErr.Code != "version_conflict" || *apiErr.ExpectedVersion != 6 || *apiErr.CurrentVersion != 7 {
		t.Errorf("unexpected %+v", apiErr)
	}
	nested := parseAPIError([]byte(`{"error":{"code":"unauthorized","message":"bad key"}}`), 401)
	if nested.Code != "unauthorized" || nested.Message != "bad key" {
		t.Errorf("unexpected %+v", nested)
	}
	raw := parseAPIError(nil, 429)
	if raw.Code != "api_error" || raw.StatusCode != 429 {
		t.Errorf("unexpected %+v", raw)
	}
}

func TestParseRetryAfter(t *testing.T) {
	now := time.Date(2026, 10, 27, 10, 0, 0, 0, time.UTC)
	cases := map[string]time.Duration{
		"":                              0,
		"3":                             3 * time.Second,
		"-1":                            0,
		"soon":                          0,
		"Tue, 27 Oct 2026 10:00:05 GMT": 5 * time.Second,
		"Tue, 27 Oct 2026 09:59:00 GMT": 0,
	}
	for value, want := range cases {
		if got := parseRetryAfter(value, now); got != want {
			t.Errorf("parseRetryAfter(%q) = %s, want %s", value, got, want)
		}
	}
}
