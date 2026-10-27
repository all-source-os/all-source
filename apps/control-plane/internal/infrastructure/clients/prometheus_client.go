package clients

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"
)

const (
	flyTokenPrefix      = "FlyV1 "
	promPartialMessage  = "partial result: some storage nodes did not answer"
	maxPromErrorBodyLen = 300
	promStatusSuccess   = "success"
)

// PromPoint is one (time, value) pair of a Prometheus result.
type PromPoint struct {
	Time  time.Time
	Value float64
}

// PromSample is one series of an instant-query vector result.
type PromSample struct {
	Labels map[string]string
	PromPoint
}

// PromSeries is one series of a range-query matrix result.
type PromSeries struct {
	Labels map[string]string
	Points []PromPoint
}

// PrometheusError reports a Prometheus HTTP API failure. Its message never
// carries the credential.
type PrometheusError struct {
	Status  int
	Message string
}

func (e *PrometheusError) Error() string {
	if e.Status == 0 {
		return "prometheus: " + e.Message
	}
	return fmt.Sprintf("prometheus: HTTP %d: %s", e.Status, e.Message)
}

// PrometheusClient reads the Prometheus HTTP API (/api/v1/query and
// /api/v1/query_range), e.g. Fly managed Prometheus.
type PrometheusClient struct {
	httpClient *http.Client
	baseURL    string
	token      string
	authHeader string
}

// NewPrometheusClient builds a client for baseURL (no trailing /api/v1). A nil
// httpClient defaults to http.DefaultClient.
func NewPrometheusClient(httpClient *http.Client, baseURL, token string) *PrometheusClient {
	if httpClient == nil {
		httpClient = http.DefaultClient
	}
	token = strings.TrimSpace(token)
	return &PrometheusClient{
		httpClient: httpClient,
		baseURL:    strings.TrimRight(baseURL, "/"),
		token:      token,
		authHeader: PrometheusAuthHeader(token),
	}
}

// flyMacaroonPrefixes mark a raw Fly token; Fly Prometheus answers 401 when one
// is sent as Bearer.
var flyMacaroonPrefixes = []string{"fm1", "fm2_", "fo1_"}

// PrometheusAuthHeader returns the Authorization value for token: a Fly token
// already carrying its "FlyV1 " scheme is sent as-is, a raw Fly macaroon gets
// the "FlyV1 " scheme, anything else is sent as Bearer.
func PrometheusAuthHeader(token string) string {
	token = strings.TrimSpace(token)
	if strings.HasPrefix(token, flyTokenPrefix) {
		return token
	}
	for _, p := range flyMacaroonPrefixes {
		if strings.HasPrefix(token, p) {
			return flyTokenPrefix + token
		}
	}
	return "Bearer " + token
}

// Query runs an instant query evaluated at at.
func (c *PrometheusClient) Query(ctx context.Context, query string, at time.Time) ([]PromSample, error) {
	q := url.Values{}
	q.Set("query", query)
	q.Set("time", formatPromTime(at))
	data, err := c.get(ctx, "/api/v1/query", q)
	if err != nil {
		return nil, err
	}
	var raw []struct {
		Metric map[string]string `json:"metric"`
		Value  []any             `json:"value"`
	}
	if err := decodePromResult(data, "vector", &raw); err != nil {
		return nil, err
	}
	out := make([]PromSample, 0, len(raw))
	for _, r := range raw {
		p, err := parsePromPoint(r.Value)
		if err != nil {
			return nil, err
		}
		out = append(out, PromSample{Labels: r.Metric, PromPoint: p})
	}
	return out, nil
}

// QueryRange runs a range query from start to end (inclusive) at step.
func (c *PrometheusClient) QueryRange(ctx context.Context, query string, start, end time.Time, step time.Duration) ([]PromSeries, error) {
	q := url.Values{}
	q.Set("query", query)
	q.Set("start", formatPromTime(start))
	q.Set("end", formatPromTime(end))
	q.Set("step", strconv.FormatInt(int64(step/time.Second), 10))
	data, err := c.get(ctx, "/api/v1/query_range", q)
	if err != nil {
		return nil, err
	}
	var raw []struct {
		Metric map[string]string `json:"metric"`
		Values [][]any           `json:"values"`
	}
	if err := decodePromResult(data, "matrix", &raw); err != nil {
		return nil, err
	}
	out := make([]PromSeries, 0, len(raw))
	for _, r := range raw {
		s := PromSeries{Labels: r.Metric, Points: make([]PromPoint, 0, len(r.Values))}
		for _, v := range r.Values {
			p, err := parsePromPoint(v)
			if err != nil {
				return nil, err
			}
			s.Points = append(s.Points, p)
		}
		out = append(out, s)
	}
	return out, nil
}

type promEnvelope struct {
	Status    string `json:"status"`
	ErrorType string `json:"errorType"`
	Error     string `json:"error"`
	IsPartial bool   `json:"isPartial"`
	Data      struct {
		ResultType string          `json:"resultType"`
		Result     json.RawMessage `json:"result"`
	} `json:"data"`
}

func (c *PrometheusClient) get(ctx context.Context, path string, query url.Values) (*promEnvelope, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, c.baseURL+path+"?"+query.Encode(), http.NoBody)
	if err != nil {
		return nil, &PrometheusError{Message: "build request failed"}
	}
	req.Header.Set("Authorization", c.authHeader)
	req.Header.Set("Accept", "application/json")

	resp, err := c.httpClient.Do(req)
	if err != nil {
		var uerr *url.Error
		if errors.As(err, &uerr) {
			err = uerr.Err
		}
		return nil, &PrometheusError{Message: c.redact(truncate(err.Error(), maxPromErrorBodyLen))}
	}
	defer func() { _ = resp.Body.Close() }() //nolint:errcheck // close-on-defer, non-actionable
	body, err := io.ReadAll(io.LimitReader(resp.Body, 32<<20))
	if err != nil {
		return nil, &PrometheusError{Status: resp.StatusCode, Message: "read body failed"}
	}
	var env promEnvelope
	decodeErr := json.Unmarshal(body, &env)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		msg := env.Error
		if decodeErr != nil || msg == "" {
			msg = string(body)
		}
		return nil, &PrometheusError{Status: resp.StatusCode, Message: c.redact(truncate(msg, maxPromErrorBodyLen))}
	}
	if decodeErr != nil {
		return nil, &PrometheusError{Status: resp.StatusCode, Message: "response is not JSON"}
	}
	if env.Status != promStatusSuccess {
		return nil, &PrometheusError{Status: resp.StatusCode, Message: c.redact(truncate(env.ErrorType+": "+env.Error, maxPromErrorBodyLen))}
	}
	if env.IsPartial {
		return nil, &PrometheusError{Status: resp.StatusCode, Message: promPartialMessage}
	}
	return &env, nil
}

func (c *PrometheusClient) redact(s string) string {
	for _, secret := range []string{c.authHeader, c.token, strings.TrimPrefix(c.token, flyTokenPrefix)} {
		if secret != "" {
			s = strings.ReplaceAll(s, secret, "[redacted]")
		}
	}
	return s
}

func decodePromResult(env *promEnvelope, wantType string, out any) error {
	if env.Data.ResultType != wantType {
		return &PrometheusError{Message: fmt.Sprintf("result type %q, want %q", env.Data.ResultType, wantType)}
	}
	if err := json.Unmarshal(env.Data.Result, out); err != nil {
		return &PrometheusError{Message: "malformed " + wantType + " result"}
	}
	return nil
}

func parsePromPoint(v []any) (PromPoint, error) {
	if len(v) != 2 {
		return PromPoint{}, &PrometheusError{Message: "malformed sample"}
	}
	ts, ok := v[0].(float64)
	if !ok {
		return PromPoint{}, &PrometheusError{Message: "malformed sample timestamp"}
	}
	s, ok := v[1].(string)
	if !ok {
		return PromPoint{}, &PrometheusError{Message: "malformed sample value"}
	}
	val, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return PromPoint{}, &PrometheusError{Message: "malformed sample value"}
	}
	sec := int64(ts)
	nsec := int64((ts - float64(sec)) * 1e9)
	return PromPoint{Time: time.Unix(sec, nsec).UTC(), Value: val}, nil
}

func formatPromTime(t time.Time) string {
	return strconv.FormatFloat(float64(t.UnixMilli())/1000, 'f', 3, 64)
}

func truncate(s string, n int) string {
	s = strings.TrimSpace(s)
	if len(s) <= n {
		return s
	}
	return s[:n] + "…"
}
