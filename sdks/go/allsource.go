// Package allsource provides a Go client for the AllSource Event Store API.
package allsource

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"net/http"
	"net/url"
	"strconv"
	"time"
)

// RetryConfig controls exponential backoff retry behavior.
type RetryConfig struct {
	// MaxRetries is the maximum number of retry attempts (default 3).
	MaxRetries int
	// BaseDelay is the initial delay before the first retry (default 200ms).
	BaseDelay time.Duration
	// BackoffFactor is the multiplier applied to the delay after each retry (default 2.0).
	BackoffFactor float64
	// MaxDelay is the maximum delay between retries (default 10s).
	MaxDelay time.Duration
}

// DefaultRetryConfig returns the default retry configuration.
func DefaultRetryConfig() RetryConfig {
	return RetryConfig{
		MaxRetries:    3,
		BaseDelay:     200 * time.Millisecond,
		BackoffFactor: 2.0,
		MaxDelay:      10 * time.Second,
	}
}

// Option is a functional option for configuring the Client.
type Option func(*Client)

// WithRetry configures retry behavior with exponential backoff.
func WithRetry(cfg RetryConfig) Option {
	return func(c *Client) {
		c.retry = &cfg
	}
}

// WithCircuitBreaker configures a circuit breaker that opens after threshold
// consecutive failures and recovers after the given duration.
func WithCircuitBreaker(threshold int, recovery time.Duration) Option {
	return func(c *Client) {
		c.cb = NewCircuitBreaker(threshold, recovery)
	}
}

// WithHTTPClient sets a custom *http.Client for the AllSource client.
func WithHTTPClient(hc *http.Client) Option {
	return func(c *Client) {
		c.http = hc
	}
}

// WithTimeout sets the HTTP client timeout.
func WithTimeout(d time.Duration) Option {
	return func(c *Client) {
		c.http.Timeout = d
	}
}

// Client is the AllSource Event Store API client.
type Client struct {
	apiKey  string
	baseURL string
	http    *http.Client
	retry   *RetryConfig
	cb      *CircuitBreaker
}

// New creates a new AllSource client. Options are applied in order after defaults.
func New(apiKey, baseURL string, opts ...Option) *Client {
	c := &Client{
		apiKey:  apiKey,
		baseURL: baseURL,
		http: &http.Client{
			Timeout: 30 * time.Second,
		},
	}
	for _, opt := range opts {
		opt(c)
	}
	return c
}

// NewWithHTTPClient creates a new AllSource client with a custom http.Client.
// Deprecated: use New with WithHTTPClient instead.
func NewWithHTTPClient(apiKey, baseURL string, httpClient *http.Client) *Client {
	return &Client{
		apiKey:  apiKey,
		baseURL: baseURL,
		http:    httpClient,
	}
}

// retryableStatusCodes are HTTP status codes that are safe to retry.
var retryableStatusCodes = map[int]bool{
	408: true,
	429: true,
	500: true,
	502: true,
	503: true,
	504: true,
}

// response is one HTTP answer. It is never nil, so a caller can read the
// status of a failed request; status is 0 when no response arrived.
type response struct {
	body   []byte
	status int
	header http.Header
}

func (c *Client) do(ctx context.Context, method, path string, body any) ([]byte, int, error) {
	resp, err := c.send(ctx, method, path, body, nil)
	if err != nil {
		return nil, resp.status, err
	}
	return resp.body, resp.status, nil
}

func (c *Client) send(ctx context.Context, method, path string, body any, header http.Header) (*response, error) {
	// Check circuit breaker before attempting the request.
	if c.cb != nil && !c.cb.Allow() {
		return &response{}, ErrCircuitOpen
	}

	maxAttempts := 1
	var retryCfg RetryConfig
	if c.retry != nil {
		retryCfg = *c.retry
		maxAttempts = retryCfg.MaxRetries + 1
	}

	var lastErr error
	last := &response{}

	for attempt := 0; attempt < maxAttempts; attempt++ {
		if attempt > 0 {
			delay := time.Duration(float64(retryCfg.BaseDelay) * math.Pow(retryCfg.BackoffFactor, float64(attempt-1)))
			if delay > retryCfg.MaxDelay {
				delay = retryCfg.MaxDelay
			}
			var apiErr *APIError
			if errors.As(lastErr, &apiErr) && apiErr.RetryAfter > delay {
				delay = apiErr.RetryAfter
			}
			select {
			case <-ctx.Done():
				return &response{}, ctx.Err()
			case <-time.After(delay):
			}
		}

		resp, err := c.doOnce(ctx, method, path, body, header)
		if err == nil {
			if c.cb != nil {
				c.cb.RecordSuccess()
			}
			return resp, nil
		}

		lastErr = err
		last = resp
		status := resp.status

		// Determine if this error is retryable.
		if !c.shouldRetry(err, status) {
			if c.cb != nil {
				// Non-retryable client errors (4xx except 408/429) are not circuit breaker failures.
				if status >= 500 || status == 408 || status == 429 {
					c.cb.RecordFailure()
				}
			}
			return last, lastErr
		}

		if c.cb != nil {
			c.cb.RecordFailure()
		}
	}

	return last, lastErr
}

func (c *Client) shouldRetry(err error, status int) bool {
	if c.retry == nil {
		return false
	}

	// Transport errors (no status code) are retryable.
	var apiErr *APIError
	if errors.As(err, &apiErr) {
		return retryableStatusCodes[apiErr.StatusCode]
	}

	// Non-APIError means transport/network failure; retryable.
	if status == 0 {
		return true
	}

	return retryableStatusCodes[status]
}

func (c *Client) doOnce(ctx context.Context, method, path string, body any, header http.Header) (*response, error) {
	var reqBody io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return &response{}, fmt.Errorf("marshal request body: %w", err)
		}
		reqBody = bytes.NewReader(b)
	}

	req, err := http.NewRequestWithContext(ctx, method, c.baseURL+path, reqBody)
	if err != nil {
		return &response{}, fmt.Errorf("create request: %w", err)
	}
	// The gateway reads only Authorization; the Query Service reads X-API-Key first.
	req.Header.Set("Authorization", "Bearer "+c.apiKey)
	req.Header.Set("X-API-Key", c.apiKey)
	req.Header.Set("Content-Type", "application/json")
	for name, values := range header {
		for _, v := range values {
			req.Header.Add(name, v)
		}
	}

	resp, err := c.http.Do(req)
	if err != nil {
		return &response{}, fmt.Errorf("execute request: %w", err)
	}
	defer resp.Body.Close()

	out := &response{status: resp.StatusCode, header: resp.Header}
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return out, fmt.Errorf("read response body: %w", err)
	}

	if resp.StatusCode >= 400 {
		apiErr := parseAPIError(data, resp.StatusCode)
		apiErr.RetryAfter = parseRetryAfter(resp.Header.Get("Retry-After"), time.Now())
		return out, apiErr
	}

	out.body = data
	return out, nil
}

// parseAPIError reads both error shapes: the gateway's
// {"error":{"code","message"}} and Core's {"error":"<code>", ...fields}.
func parseAPIError(data []byte, statusCode int) *APIError {
	var errResp struct {
		Error           json.RawMessage `json:"error"`
		Message         string          `json:"message"`
		Detail          string          `json:"detail"`
		ExpectedVersion *int64          `json:"expected_version"`
		CurrentVersion  *int64          `json:"current_version"`
		Generation      *int64          `json:"generation"`
		Ready           *bool           `json:"ready"`
	}
	if err := json.Unmarshal(data, &errResp); err == nil && len(errResp.Error) > 0 {
		apiErr := &APIError{
			StatusCode:      statusCode,
			Detail:          errResp.Detail,
			ExpectedVersion: errResp.ExpectedVersion,
			CurrentVersion:  errResp.CurrentVersion,
			Generation:      errResp.Generation,
			Ready:           errResp.Ready,
		}
		var nested struct {
			Code    string `json:"code"`
			Message string `json:"message"`
		}
		if json.Unmarshal(errResp.Error, &nested) == nil && nested.Code != "" {
			apiErr.Code = nested.Code
			apiErr.Message = nested.Message
			return apiErr
		}
		var code string
		if json.Unmarshal(errResp.Error, &code) == nil && code != "" {
			apiErr.Code = code
			apiErr.Message = errResp.Message
			if apiErr.Message == "" {
				apiErr.Message = errResp.Detail
			}
			return apiErr
		}
	}
	return &APIError{
		Code:       "api_error",
		Message:    string(data),
		StatusCode: statusCode,
	}
}

// parseRetryAfter accepts both RFC 9110 forms: delay-seconds and an HTTP date.
func parseRetryAfter(value string, now time.Time) time.Duration {
	if value == "" {
		return 0
	}
	if secs, err := strconv.Atoi(value); err == nil {
		if secs < 0 {
			return 0
		}
		return time.Duration(secs) * time.Second
	}
	if at, err := http.ParseTime(value); err == nil {
		if d := at.Sub(now); d > 0 {
			return d
		}
	}
	return 0
}

// IngestResponse is the response from ingesting a single event.
type IngestResponse struct {
	EventID   string `json:"event_id"`
	Timestamp string `json:"timestamp"`
}

// BatchIngestResponse is the response from batch ingesting events.
type BatchIngestResponse struct {
	Total    int              `json:"total"`
	Ingested int              `json:"ingested"`
	Events   []IngestResponse `json:"events"`
}

// Ingest sends a single event to the AllSource Event Store.
func (c *Client) Ingest(ctx context.Context, eventType, entityID string, data map[string]any) (*IngestResponse, error) {
	body := map[string]any{
		"event_type": eventType,
		"entity_id":  entityID,
		"payload":    data,
	}
	respData, _, err := c.do(ctx, http.MethodPost, "/api/v1/events", body)
	if err != nil {
		return nil, err
	}

	var resp IngestResponse
	if err := json.Unmarshal(respData, &resp); err != nil {
		return nil, fmt.Errorf("decode ingest response: %w", err)
	}
	return &resp, nil
}

// IngestBatch sends multiple events in a single request.
func (c *Client) IngestBatch(ctx context.Context, events []map[string]any) (*BatchIngestResponse, error) {
	body := map[string]any{
		"events": events,
	}
	respData, _, err := c.do(ctx, http.MethodPost, "/api/v1/events/batch", body)
	if err != nil {
		return nil, err
	}

	var resp BatchIngestResponse
	if err := json.Unmarshal(respData, &resp); err != nil {
		return nil, fmt.Errorf("decode batch ingest response: %w", err)
	}
	return &resp, nil
}

// QueryOptions specifies filters for querying events.
type QueryOptions struct {
	EventType string
	EntityID  string
	Start     string // ISO-8601 timestamp, maps to "since" query param
	End       string // ISO-8601 timestamp, maps to "until" query param
	Limit     int
	Offset    int
	// IfNoneMatch is an ETag from an earlier EventList; an unchanged result
	// returns a *NotModifiedError (errors.Is(err, ErrNotModified)).
	IfNoneMatch string
}

// Query retrieves events matching the given filters.
func (c *Client) Query(ctx context.Context, opts QueryOptions) (*EventList, error) {
	params := url.Values{}
	if opts.EventType != "" {
		params.Set("event_type", opts.EventType)
	}
	if opts.EntityID != "" {
		params.Set("entity_id", opts.EntityID)
	}
	if opts.Start != "" {
		params.Set("since", opts.Start)
	}
	if opts.End != "" {
		params.Set("until", opts.End)
	}
	if opts.Limit > 0 {
		params.Set("limit", strconv.Itoa(opts.Limit))
	}
	if opts.Offset > 0 {
		params.Set("offset", strconv.Itoa(opts.Offset))
	}

	var el EventList
	etag, err := c.getJSON(ctx, withQuery("/api/v1/events/query", params), opts.IfNoneMatch, "event list", &el)
	if err != nil {
		return nil, err
	}
	el.ETag = etag
	return &el, nil
}

// getJSON performs a conditional GET. A 304 returns a *NotModifiedError
// carrying the ETag; any other success decodes into out.
func (c *Client) getJSON(ctx context.Context, path, ifNoneMatch, what string, out any) (string, error) {
	var header http.Header
	if ifNoneMatch != "" {
		header = http.Header{"If-None-Match": []string{ifNoneMatch}}
	}
	resp, err := c.send(ctx, http.MethodGet, path, nil, header)
	if err != nil {
		return "", err
	}
	etag := resp.header.Get("ETag")
	if resp.status == http.StatusNotModified {
		return etag, &NotModifiedError{ETag: etag}
	}
	if err := json.Unmarshal(resp.body, out); err != nil {
		return etag, fmt.Errorf("decode %s: %w", what, err)
	}
	return etag, nil
}

func withQuery(path string, params url.Values) string {
	if len(params) == 0 {
		return path
	}
	return path + "?" + params.Encode()
}

// PrimeProjection describes a Prime projection definition: the entity type and
// the per-field merge policies that govern how observations fold into fields.
type PrimeProjection struct {
	EntityType    string            `json:"entity_type"`
	FieldPolicies map[string]string `json:"field_policies"`
}

// PrimeProjectionAck is the acknowledgement returned when a Prime projection is
// defined.
type PrimeProjectionAck struct {
	EntityType string `json:"entity_type"`
	Persisted  bool   `json:"persisted"`
}

// PrimeSnapshot is the materialized field view of a Prime node, produced by
// folding its observations through the entity type's merge policies.
type PrimeSnapshot struct {
	EntityType       string         `json:"entity_type"`
	Fields           map[string]any `json:"fields"`
	ObservationCount int            `json:"observation_count"`
}

// PrimeProvenance describes which source event supplied a node field's current
// value and the merge policy that selected it.
type PrimeProvenance struct {
	Field              string `json:"field"`
	Value              any    `json:"value"`
	SourceEventID      string `json:"source_event_id"`
	SourceEventAt      string `json:"source_event_at"`
	MergePolicyApplied string `json:"merge_policy_applied"`
}

// GetPrimeProjections returns all Prime projection definitions.
func (c *Client) GetPrimeProjections(ctx context.Context) ([]PrimeProjection, error) {
	respData, _, err := c.do(ctx, http.MethodGet, "/api/v1/prime/projections", nil)
	if err != nil {
		return nil, err
	}

	var w struct {
		Data []PrimeProjection `json:"data"`
	}
	if err := json.Unmarshal(respData, &w); err != nil {
		return nil, fmt.Errorf("decode prime projections: %w", err)
	}
	return w.Data, nil
}

// DefinePrimeProjection defines (or updates) a Prime projection for an entity
// type with the given per-field merge policies (e.g. last_write,
// highest_priority, most_specific, merge_array).
func (c *Client) DefinePrimeProjection(ctx context.Context, entityType string, fieldPolicies map[string]string) (*PrimeProjectionAck, error) {
	body := map[string]any{
		"entity_type":    entityType,
		"field_policies": fieldPolicies,
	}
	respData, _, err := c.do(ctx, http.MethodPost, "/api/v1/prime/projections", body)
	if err != nil {
		return nil, err
	}

	var w struct {
		Data PrimeProjectionAck `json:"data"`
	}
	if err := json.Unmarshal(respData, &w); err != nil {
		return nil, fmt.Errorf("decode prime projection ack: %w", err)
	}
	return &w.Data, nil
}

// ProjectNode materializes the current field snapshot for a Prime node by
// folding its observations through the entity type's merge policies.
func (c *Client) ProjectNode(ctx context.Context, nodeID string) (*PrimeSnapshot, error) {
	path := fmt.Sprintf("/api/v1/prime/nodes/%s/project", nodeID)
	respData, _, err := c.do(ctx, http.MethodPost, path, nil)
	if err != nil {
		return nil, err
	}

	var w struct {
		Data PrimeSnapshot `json:"data"`
	}
	if err := json.Unmarshal(respData, &w); err != nil {
		return nil, fmt.Errorf("decode prime snapshot: %w", err)
	}
	return &w.Data, nil
}

// NodeFieldProvenance returns the provenance for a single field of a Prime node:
// which source event supplied the current value and the merge policy applied.
// A 404 propagates as an *APIError; callers can check apiErr.IsNotFound().
func (c *Client) NodeFieldProvenance(ctx context.Context, nodeID, field string) (*PrimeProvenance, error) {
	path := fmt.Sprintf("/api/v1/prime/nodes/%s/fields/%s/provenance", nodeID, field)
	respData, _, err := c.do(ctx, http.MethodGet, path, nil)
	if err != nil {
		return nil, err
	}

	var w struct {
		Data PrimeProvenance `json:"data"`
	}
	if err := json.Unmarshal(respData, &w); err != nil {
		return nil, fmt.Errorf("decode prime provenance: %w", err)
	}
	return &w.Data, nil
}

// Health checks the API health endpoint.
func (c *Client) Health(ctx context.Context) (map[string]any, error) {
	respData, _, err := c.do(ctx, http.MethodGet, "/health", nil)
	if err != nil {
		return nil, err
	}

	var result map[string]any
	if err := json.Unmarshal(respData, &result); err != nil {
		return nil, fmt.Errorf("decode health response: %w", err)
	}
	return result, nil
}
