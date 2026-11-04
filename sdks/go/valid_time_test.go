package allsource

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"testing"
)

func TestIngestValidTimeSendsFieldsOnlyWhenSet(t *testing.T) {
	var bodies []map[string]any
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		raw, _ := io.ReadAll(r.Body)
		var body map[string]any
		if err := json.Unmarshal(raw, &body); err != nil {
			t.Errorf("body is not JSON: %v", err)
		}
		bodies = append(bodies, body)
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"event_id":"e1","timestamp":"2026-11-04T10:00:00Z","valid_from":"2026-09-01T00:00:00Z","recorded_at":"2026-11-04T10:00:00Z"}`))
	})
	defer srv.Close()

	plain, err := client.Ingest(context.Background(), "a.b", "e1", map[string]any{})
	if err != nil {
		t.Fatalf("Ingest: %v", err)
	}
	if _, has := bodies[0]["valid_from"]; has {
		t.Errorf("valid_from sent without being set: %v", bodies[0])
	}
	if plain.EventID != "e1" {
		t.Errorf("unexpected ack %+v", plain)
	}

	dated, err := client.IngestValidTime(context.Background(), "skill.published", "skill:abc", map[string]any{},
		ValidTime{From: "2026-09-01T00:00:00Z", To: "2026-09-30T00:00:00Z"})
	if err != nil {
		t.Fatalf("IngestValidTime: %v", err)
	}
	if bodies[1]["valid_from"] != "2026-09-01T00:00:00Z" || bodies[1]["valid_to"] != "2026-09-30T00:00:00Z" {
		t.Errorf("valid time not sent: %v", bodies[1])
	}
	if dated.ValidFrom != "2026-09-01T00:00:00Z" || dated.RecordedAt != "2026-11-04T10:00:00Z" {
		t.Errorf("unexpected ack %+v", dated)
	}
}

func TestQueryAndEntityStateSendEachValidTimeBoundUnderItsOwnKey(t *testing.T) {
	var queries []map[string]string
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query()
		seen := map[string]string{}
		for key := range q {
			seen[key] = q.Get(key)
		}
		queries = append(queries, seen)
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(`{"events":[],"count":0,"entity_id":"skill:abc","current_state":{},"event_count":0,"version":0}`))
	})
	defer srv.Close()

	_, err := client.Query(context.Background(), QueryOptions{
		AsOf:       "2026-10-01T00:00:00Z",
		Start:      "2026-01-01T00:00:00Z",
		AsOfValid:  "2026-09-15T00:00:00Z",
		SinceValid: "2026-09-01T00:00:00Z",
		UntilValid: "2026-09-30T00:00:00Z",
	})
	if err != nil {
		t.Fatalf("Query: %v", err)
	}
	want := map[string]string{
		"as_of":       "2026-10-01T00:00:00Z",
		"since":       "2026-01-01T00:00:00Z",
		"as_of_valid": "2026-09-15T00:00:00Z",
		"since_valid": "2026-09-01T00:00:00Z",
		"until_valid": "2026-09-30T00:00:00Z",
	}
	for key, value := range want {
		if queries[0][key] != value {
			t.Errorf("query %s = %q, want %q (all: %v)", key, queries[0][key], value, queries[0])
		}
	}
	if _, has := queries[0]["until"]; has {
		t.Errorf("until sent for a valid-time-only bound: %v", queries[0])
	}

	_, err = client.EntityState(context.Background(), "skill:abc", EntityStateOptions{AsOfValid: "2026-09-15T00:00:00Z"})
	if err != nil {
		t.Fatalf("EntityState: %v", err)
	}
	if queries[1]["as_of_valid"] != "2026-09-15T00:00:00Z" {
		t.Errorf("entity state as_of_valid = %q", queries[1]["as_of_valid"])
	}
	if _, has := queries[1]["as_of"]; has {
		t.Errorf("as_of sent when only as_of_valid was set: %v", queries[1])
	}
}
