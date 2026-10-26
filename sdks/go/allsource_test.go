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

func setupTestServer(handler http.HandlerFunc) (*httptest.Server, *Client) {
	srv := httptest.NewServer(handler)
	client := New("test-api-key", srv.URL)
	return srv, client
}

func TestNew(t *testing.T) {
	c := New("my-key", "https://api.example.com")
	if c.apiKey != "my-key" {
		t.Errorf("expected apiKey my-key, got %s", c.apiKey)
	}
	if c.baseURL != "https://api.example.com" {
		t.Errorf("expected baseURL https://api.example.com, got %s", c.baseURL)
	}
}

func TestNewWithOptions(t *testing.T) {
	hc := &http.Client{Timeout: 5 * time.Second}
	retryCfg := RetryConfig{MaxRetries: 2, BaseDelay: 100 * time.Millisecond, BackoffFactor: 1.5, MaxDelay: 5 * time.Second}

	c := New("key", "http://localhost",
		WithHTTPClient(hc),
		WithRetry(retryCfg),
		WithCircuitBreaker(3, 15*time.Second),
		WithTimeout(10*time.Second),
	)

	if c.http != hc {
		t.Error("expected custom HTTP client")
	}
	if c.http.Timeout != 10*time.Second {
		t.Errorf("expected timeout 10s, got %s", c.http.Timeout)
	}
	if c.retry == nil {
		t.Fatal("expected retry config")
	}
	if c.retry.MaxRetries != 2 {
		t.Errorf("expected max retries 2, got %d", c.retry.MaxRetries)
	}
	if c.cb == nil {
		t.Fatal("expected circuit breaker")
	}
}

func TestIngest(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("expected POST, got %s", r.Method)
		}
		if r.URL.Path != "/api/v1/events" {
			t.Errorf("expected /api/v1/events, got %s", r.URL.Path)
		}
		if r.Header.Get("Authorization") != "Bearer test-api-key" {
			t.Errorf("expected Authorization Bearer test-api-key, got %s", r.Header.Get("Authorization"))
		}
		if r.Header.Get("X-API-Key") != "test-api-key" {
			t.Errorf("expected X-API-Key test-api-key, got %s", r.Header.Get("X-API-Key"))
		}

		var body map[string]any
		json.NewDecoder(r.Body).Decode(&body)
		if body["event_type"] != "user.signup" {
			t.Errorf("expected event_type user.signup, got %v", body["event_type"])
		}
		if body["entity_id"] != "user-123" {
			t.Errorf("expected entity_id user-123, got %v", body["entity_id"])
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"event_id":  "evt-1",
			"timestamp": "2026-01-01T00:00:00Z",
		})
	})
	defer srv.Close()

	resp, err := client.Ingest(context.Background(), "user.signup", "user-123", map[string]any{"plan": "pro"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp.EventID != "evt-1" {
		t.Errorf("expected EventID evt-1, got %s", resp.EventID)
	}
}

func TestQuery(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			t.Errorf("expected GET, got %s", r.Method)
		}
		if r.URL.Path != "/api/v1/events/query" {
			t.Errorf("expected /api/v1/events/query, got %s", r.URL.Path)
		}
		if r.URL.Query().Get("event_type") != "user.signup" {
			t.Errorf("expected event_type=user.signup, got %s", r.URL.Query().Get("event_type"))
		}
		if r.URL.Query().Get("limit") != "10" {
			t.Errorf("expected limit=10, got %s", r.URL.Query().Get("limit"))
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"events": []map[string]any{
				{
					"id":         "evt-1",
					"entity_id":  "user-123",
					"event_type": "user.signup",
					"payload":    map[string]any{},
					"timestamp":  "2026-01-01T00:00:00Z",
					"version":    1,
				},
			},
			"count": 1,
		})
	})
	defer srv.Close()

	result, err := client.Query(context.Background(), QueryOptions{
		EventType: "user.signup",
		Limit:     10,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Count != 1 {
		t.Errorf("expected count 1, got %d", result.Count)
	}
	if len(result.Events) != 1 {
		t.Errorf("expected 1 event, got %d", len(result.Events))
	}
	if result.Events[0].ID != "evt-1" {
		t.Errorf("expected event ID evt-1, got %s", result.Events[0].ID)
	}
}

func TestGetPrimeProjections(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			t.Errorf("expected GET, got %s", r.Method)
		}
		if r.URL.Path != "/api/v1/prime/projections" {
			t.Errorf("expected /api/v1/prime/projections, got %s", r.URL.Path)
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"data": []map[string]any{
				{
					"entity_type": "person",
					"field_policies": map[string]any{
						"name":  "last_write",
						"email": "highest_priority",
					},
				},
			},
			"count": 1,
		})
	})
	defer srv.Close()

	result, err := client.GetPrimeProjections(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(result) != 1 {
		t.Fatalf("expected 1 projection, got %d", len(result))
	}
	if result[0].EntityType != "person" {
		t.Errorf("expected entity_type person, got %s", result[0].EntityType)
	}
	if result[0].FieldPolicies["name"] != "last_write" {
		t.Errorf("expected name policy last_write, got %s", result[0].FieldPolicies["name"])
	}
	if result[0].FieldPolicies["email"] != "highest_priority" {
		t.Errorf("expected email policy highest_priority, got %s", result[0].FieldPolicies["email"])
	}
}

func TestDefinePrimeProjection(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("expected POST, got %s", r.Method)
		}
		if r.URL.Path != "/api/v1/prime/projections" {
			t.Errorf("expected /api/v1/prime/projections, got %s", r.URL.Path)
		}

		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Fatalf("decode request body: %v", err)
		}
		if body["entity_type"] != "person" {
			t.Errorf("expected entity_type person, got %v", body["entity_type"])
		}
		policies, ok := body["field_policies"].(map[string]any)
		if !ok {
			t.Fatalf("expected field_policies map, got %T", body["field_policies"])
		}
		if policies["tags"] != "merge_array" {
			t.Errorf("expected tags policy merge_array, got %v", policies["tags"])
		}

		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusCreated)
		json.NewEncoder(w).Encode(map[string]any{
			"data": map[string]any{
				"entity_type": "person",
				"persisted":   true,
			},
		})
	})
	defer srv.Close()

	ack, err := client.DefinePrimeProjection(context.Background(), "person", map[string]string{
		"tags": "merge_array",
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if ack.EntityType != "person" {
		t.Errorf("expected entity_type person, got %s", ack.EntityType)
	}
	if !ack.Persisted {
		t.Error("expected persisted true")
	}
}

func TestProjectNode(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			t.Errorf("expected POST, got %s", r.Method)
		}
		if r.URL.Path != "/api/v1/prime/nodes/person:alice/project" {
			t.Errorf("expected /api/v1/prime/nodes/person:alice/project, got %s", r.URL.Path)
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"data": map[string]any{
				"entity_type": "person",
				"fields": map[string]any{
					"name": "Alice",
				},
				"observation_count": 3,
			},
		})
	})
	defer srv.Close()

	snap, err := client.ProjectNode(context.Background(), "person:alice")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if snap.EntityType != "person" {
		t.Errorf("expected entity_type person, got %s", snap.EntityType)
	}
	if snap.Fields["name"] != "Alice" {
		t.Errorf("expected field name Alice, got %v", snap.Fields["name"])
	}
	if snap.ObservationCount != 3 {
		t.Errorf("expected observation_count 3, got %d", snap.ObservationCount)
	}
}

func TestNodeFieldProvenance(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			t.Errorf("expected GET, got %s", r.Method)
		}
		if r.URL.Path != "/api/v1/prime/nodes/person:alice/fields/name/provenance" {
			t.Errorf("expected /api/v1/prime/nodes/person:alice/fields/name/provenance, got %s", r.URL.Path)
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"data": map[string]any{
				"field":                "name",
				"value":                "Alice",
				"source_event_id":      "evt-42",
				"source_event_at":      "2026-01-01T00:00:00Z",
				"merge_policy_applied": "last_write",
			},
		})
	})
	defer srv.Close()

	prov, err := client.NodeFieldProvenance(context.Background(), "person:alice", "name")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if prov.Field != "name" {
		t.Errorf("expected field name, got %s", prov.Field)
	}
	if prov.Value != "Alice" {
		t.Errorf("expected value Alice, got %v", prov.Value)
	}
	if prov.SourceEventID != "evt-42" {
		t.Errorf("expected source_event_id evt-42, got %s", prov.SourceEventID)
	}
	if prov.MergePolicyApplied != "last_write" {
		t.Errorf("expected merge_policy_applied last_write, got %s", prov.MergePolicyApplied)
	}
}

func TestNodeFieldProvenanceNotFound(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusNotFound)
		json.NewEncoder(w).Encode(map[string]any{
			"error": map[string]any{
				"code":    "not_found",
				"message": "no provenance for field",
			},
		})
	})
	defer srv.Close()

	_, err := client.NodeFieldProvenance(context.Background(), "person:alice", "name")
	if err == nil {
		t.Fatal("expected error, got nil")
	}

	var apiErr *APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("expected *APIError, got %T", err)
	}
	if !apiErr.IsNotFound() {
		t.Errorf("expected IsNotFound, got status %d", apiErr.StatusCode)
	}
}

func TestHealth(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/health" {
			t.Errorf("expected /health, got %s", r.URL.Path)
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"status": "healthy",
		})
	})
	defer srv.Close()

	result, err := client.Health(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result["status"] != "healthy" {
		t.Errorf("expected status healthy, got %v", result["status"])
	}
}

func TestAPIError401(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(401)
		json.NewEncoder(w).Encode(map[string]any{
			"error": map[string]any{
				"code":    "unauthorized",
				"message": "invalid api key",
			},
		})
	})
	defer srv.Close()

	_, err := client.Health(context.Background())
	if err == nil {
		t.Fatal("expected error, got nil")
	}

	var apiErr *APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("expected *APIError, got %T", err)
	}
	if !apiErr.IsUnauthorized() {
		t.Errorf("expected IsUnauthorized, got status %d", apiErr.StatusCode)
	}
}

func TestAPIError429(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(429)
		json.NewEncoder(w).Encode(map[string]any{
			"error": map[string]any{
				"code":    "rate_limited",
				"message": "too many requests",
			},
		})
	})
	defer srv.Close()

	_, err := client.Health(context.Background())
	var apiErr *APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("expected *APIError, got %T", err)
	}
	if !apiErr.IsRateLimited() {
		t.Errorf("expected IsRateLimited, got status %d", apiErr.StatusCode)
	}
}

func TestQueryWithAllOptions(t *testing.T) {
	srv, client := setupTestServer(func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query()
		if q.Get("event_type") != "order.placed" {
			t.Errorf("expected event_type=order.placed, got %s", q.Get("event_type"))
		}
		if q.Get("entity_id") != "order-456" {
			t.Errorf("expected entity_id=order-456, got %s", q.Get("entity_id"))
		}
		if q.Get("since") != "2026-01-01T00:00:00Z" {
			t.Errorf("expected since=2026-01-01T00:00:00Z, got %s", q.Get("since"))
		}
		if q.Get("until") != "2026-02-01T00:00:00Z" {
			t.Errorf("expected until=2026-02-01T00:00:00Z, got %s", q.Get("until"))
		}
		if q.Get("limit") != "50" {
			t.Errorf("expected limit=50, got %s", q.Get("limit"))
		}
		if q.Get("offset") != "10" {
			t.Errorf("expected offset=10, got %s", q.Get("offset"))
		}

		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{
			"events": []any{},
			"count":  0,
		})
	})
	defer srv.Close()

	result, err := client.Query(context.Background(), QueryOptions{
		EventType: "order.placed",
		EntityID:  "order-456",
		Start:     "2026-01-01T00:00:00Z",
		End:       "2026-02-01T00:00:00Z",
		Limit:     50,
		Offset:    10,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Count != 0 {
		t.Errorf("expected count 0, got %d", result.Count)
	}
}

func TestAPIErrorMessage(t *testing.T) {
	err := &APIError{
		Code:       "unauthorized",
		Message:    "invalid api key",
		StatusCode: 401,
	}
	expected := "allsource: unauthorized (status 401): invalid api key"
	if err.Error() != expected {
		t.Errorf("expected %q, got %q", expected, err.Error())
	}
}

func TestAPIErrorIsServerError(t *testing.T) {
	tests := []struct {
		status int
		want   bool
	}{
		{400, false},
		{404, false},
		{499, false},
		{500, true},
		{502, true},
		{503, true},
	}
	for _, tt := range tests {
		err := &APIError{StatusCode: tt.status}
		if got := err.IsServerError(); got != tt.want {
			t.Errorf("IsServerError(%d) = %v, want %v", tt.status, got, tt.want)
		}
	}
}

func TestAPIErrorIsRetryable(t *testing.T) {
	tests := []struct {
		status int
		want   bool
	}{
		{400, false},
		{401, false},
		{403, false},
		{404, false},
		{408, true},
		{429, true},
		{500, true},
		{502, true},
		{503, true},
		{504, true},
	}
	for _, tt := range tests {
		err := &APIError{StatusCode: tt.status}
		if got := err.IsRetryable(); got != tt.want {
			t.Errorf("IsRetryable(%d) = %v, want %v", tt.status, got, tt.want)
		}
	}
}

// --- Retry tests ---

func TestRetryOn503(t *testing.T) {
	var attempts atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		n := attempts.Add(1)
		if n <= 2 {
			w.WriteHeader(503)
			json.NewEncoder(w).Encode(map[string]any{
				"error": map[string]any{"code": "unavailable", "message": "try again"},
			})
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{"status": "healthy"})
	}))
	defer srv.Close()

	client := New("key", srv.URL, WithRetry(RetryConfig{
		MaxRetries:    3,
		BaseDelay:     10 * time.Millisecond,
		BackoffFactor: 1.0,
		MaxDelay:      100 * time.Millisecond,
	}))

	result, err := client.Health(context.Background())
	if err != nil {
		t.Fatalf("expected success after retries, got: %v", err)
	}
	if result["status"] != "healthy" {
		t.Errorf("expected healthy, got %v", result["status"])
	}
	if attempts.Load() != 3 {
		t.Errorf("expected 3 attempts, got %d", attempts.Load())
	}
}

func TestNoRetryOn401(t *testing.T) {
	var attempts atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts.Add(1)
		w.WriteHeader(401)
		json.NewEncoder(w).Encode(map[string]any{
			"error": map[string]any{"code": "unauthorized", "message": "bad key"},
		})
	}))
	defer srv.Close()

	client := New("key", srv.URL, WithRetry(RetryConfig{
		MaxRetries:    3,
		BaseDelay:     10 * time.Millisecond,
		BackoffFactor: 1.0,
		MaxDelay:      100 * time.Millisecond,
	}))

	_, err := client.Health(context.Background())
	if err == nil {
		t.Fatal("expected error")
	}
	if attempts.Load() != 1 {
		t.Errorf("expected 1 attempt (no retry on 401), got %d", attempts.Load())
	}
}

func TestRetryExhausted(t *testing.T) {
	var attempts atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		attempts.Add(1)
		w.WriteHeader(503)
		json.NewEncoder(w).Encode(map[string]any{
			"error": map[string]any{"code": "unavailable", "message": "down"},
		})
	}))
	defer srv.Close()

	client := New("key", srv.URL, WithRetry(RetryConfig{
		MaxRetries:    2,
		BaseDelay:     10 * time.Millisecond,
		BackoffFactor: 1.0,
		MaxDelay:      100 * time.Millisecond,
	}))

	_, err := client.Health(context.Background())
	if err == nil {
		t.Fatal("expected error after retries exhausted")
	}
	// 1 initial + 2 retries = 3
	if attempts.Load() != 3 {
		t.Errorf("expected 3 attempts, got %d", attempts.Load())
	}
}

// --- Circuit breaker integration tests ---

func TestClientCircuitBreakerOpensOnFailures(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(500)
		json.NewEncoder(w).Encode(map[string]any{
			"error": map[string]any{"code": "internal", "message": "fail"},
		})
	}))
	defer srv.Close()

	client := New("key", srv.URL, WithCircuitBreaker(3, 30*time.Second))

	// First 3 requests fail and trip the breaker.
	for i := 0; i < 3; i++ {
		_, err := client.Health(context.Background())
		if err == nil {
			t.Fatalf("expected error on attempt %d", i+1)
		}
	}

	// Fourth request should be rejected by circuit breaker without hitting the server.
	_, err := client.Health(context.Background())
	if !errors.Is(err, ErrCircuitOpen) {
		t.Errorf("expected ErrCircuitOpen, got %v", err)
	}
}

func TestClientCircuitBreakerResetsOnSuccess(t *testing.T) {
	var failCount atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		n := failCount.Load()
		if n < 2 {
			failCount.Add(1)
			w.WriteHeader(500)
			json.NewEncoder(w).Encode(map[string]any{
				"error": map[string]any{"code": "internal", "message": "fail"},
			})
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]any{"status": "healthy"})
	}))
	defer srv.Close()

	client := New("key", srv.URL, WithCircuitBreaker(5, 30*time.Second))

	// Two failures.
	for i := 0; i < 2; i++ {
		client.Health(context.Background())
	}
	if client.cb.ConsecutiveFailures() != 2 {
		t.Errorf("expected 2 failures, got %d", client.cb.ConsecutiveFailures())
	}

	// Success resets.
	_, err := client.Health(context.Background())
	if err != nil {
		t.Fatalf("expected success, got %v", err)
	}
	if client.cb.ConsecutiveFailures() != 0 {
		t.Errorf("expected 0 failures after success, got %d", client.cb.ConsecutiveFailures())
	}
	if client.cb.State() != CircuitClosed {
		t.Errorf("expected CircuitClosed, got %v", client.cb.State())
	}
}
