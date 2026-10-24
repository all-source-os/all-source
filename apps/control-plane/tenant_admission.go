package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"math/rand/v2"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/prometheus/client_golang/prometheus"
)

type admissionMode string

const (
	admissionOff     admissionMode = "off"
	admissionShadow  admissionMode = "shadow"
	admissionEnforce admissionMode = "enforce"

	admissionModeEnv      = "ALLSOURCE_ADMISSION_MODE"
	admissionOverridesEnv = "ALLSOURCE_ADMISSION_OVERRIDES"

	admissionLaneData  = "data"
	admissionLaneMeta  = "meta"
	admissionLaneWrite = "write"

	admissionScopeTenant    = "tenant"
	admissionScopeFairShare = "global_fair_share"
	admissionScopeGlobal    = "global"

	fairShareOccupancy  = 12
	fairShareMinimum    = 4
	maxRetryAfter       = 60
	maxTrackedTenants   = 4096
	idleTenantRetention = 10 * time.Minute
)

var admissionWouldRefuse = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "admission_would_refuse_total",
	Help: "Requests per-tenant admission would refuse, counted in shadow and enforce modes.",
}, []string{"lane", "scope", "tenant"})

var admissionRefused = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "admission_refused_total",
	Help: "Requests refused by admission or the global response pool.",
}, []string{"lane", "scope", "tenant"})

// admissionJitter spreads retries from clients refused in the same instant.
var admissionJitter = func() int { return rand.IntN(2) } //nolint:gosec // jitter, not a secret

type admissionLimits struct {
	DataPerSecond  float64 `json:"data_per_second"`
	DataBurst      float64 `json:"data_burst"`
	MetaPerSecond  float64 `json:"meta_per_second"`
	MetaBurst      float64 `json:"meta_burst"`
	WritePerSecond float64 `json:"write_per_second"`
	WriteBurst     float64 `json:"write_burst"`
	BytesPerSecond float64 `json:"bytes_per_second"`
	BytesBurst     float64 `json:"bytes_burst"`
	InFlight       int     `json:"in_flight"`
}

func defaultAdmissionLimits() admissionLimits {
	return admissionLimits{
		DataPerSecond: 10, DataBurst: 20,
		MetaPerSecond: 30, MetaBurst: 60,
		WritePerSecond: 100, WriteBurst: 200,
		BytesPerSecond: 512 * 1024, BytesBurst: maxEgressResponse,
		InFlight: 8,
	}
}

func (l admissionLimits) validate() error {
	for name, value := range map[string]float64{
		"data_per_second": l.DataPerSecond, "data_burst": l.DataBurst,
		"meta_per_second": l.MetaPerSecond, "meta_burst": l.MetaBurst,
		"write_per_second": l.WritePerSecond, "write_burst": l.WriteBurst,
		"bytes_per_second": l.BytesPerSecond, "bytes_burst": l.BytesBurst,
	} {
		if !(value > 0) || math.IsInf(value, 0) {
			return fmt.Errorf("%s must be a positive number", name)
		}
	}
	if l.DataBurst < 1 || l.MetaBurst < 1 || l.WriteBurst < 1 {
		return errors.New("lane bursts must admit at least one request")
	}
	if l.InFlight < 1 {
		return errors.New("in_flight must be at least 1")
	}
	return nil
}

type tokenBucket struct {
	tokens   float64
	capacity float64
	rate     float64
	updated  time.Time
}

func newTokenBucket(capacity, rate float64, now time.Time) tokenBucket {
	return tokenBucket{tokens: capacity, capacity: capacity, rate: rate, updated: now}
}

func (b *tokenBucket) refill(now time.Time) {
	if elapsed := now.Sub(b.updated).Seconds(); elapsed > 0 {
		b.tokens = math.Min(b.capacity, b.tokens+elapsed*b.rate)
	}
	b.updated = now
}

func (b *tokenBucket) deficit(n float64) float64 { return math.Max(0, n-b.tokens) }

// add moves tokens by delta; debt is floored at one bucket's capacity.
func (b *tokenBucket) add(delta float64) {
	b.tokens = math.Max(-b.capacity, math.Min(b.capacity, b.tokens+delta))
}

type tenantAdmissionState struct {
	limits      admissionLimits
	data        tokenBucket
	meta        tokenBucket
	write       tokenBucket
	bytes       tokenBucket
	inFlight    int
	lastSeen    time.Time
	wouldRefuse map[string]uint64
}

func (s *tenantAdmissionState) bucket(lane string) *tokenBucket {
	switch lane {
	case admissionLaneMeta:
		return &s.meta
	case admissionLaneWrite:
		return &s.write
	default:
		return &s.data
	}
}

func (s *tenantAdmissionState) refill(now time.Time) {
	for _, b := range []*tokenBucket{&s.data, &s.meta, &s.write, &s.bytes} {
		b.refill(now)
	}
}

type tenantAdmission struct {
	mu        sync.Mutex
	mode      admissionMode
	defaults  admissionLimits
	overrides map[string]admissionLimits
	tenants   map[string]*tenantAdmissionState
}

type admissionRefusal struct {
	status     int
	scope      string
	lane       string
	retryAfter int
}

type admissionTicket struct {
	admission *tenantAdmission
	tenant    string
	lane      string
	tookToken bool
}

func newTenantAdmissionFromEnv(lookup func(string) (string, bool)) (*tenantAdmission, error) {
	rawMode, _ := lookup(admissionModeEnv)
	mode := admissionMode(strings.ToLower(strings.TrimSpace(rawMode)))
	if mode == "" {
		mode = admissionShadow
	}
	if mode != admissionOff && mode != admissionShadow && mode != admissionEnforce {
		return nil, fmt.Errorf("%s must be off, shadow or enforce, got %q", admissionModeEnv, rawMode)
	}
	raw, present := lookup(admissionOverridesEnv)
	if mode == admissionEnforce && (!present || strings.TrimSpace(raw) == "") {
		return nil, fmt.Errorf("%s=enforce requires %s (a JSON object, {} for none)", admissionModeEnv, admissionOverridesEnv)
	}
	overrides, err := parseAdmissionOverrides(raw)
	if err != nil {
		return nil, err
	}
	return newTenantAdmission(mode, defaultAdmissionLimits(), overrides), nil
}

// parseAdmissionOverrides merges each tenant's partial object over the defaults.
func parseAdmissionOverrides(raw string) (map[string]admissionLimits, error) {
	overrides := map[string]admissionLimits{}
	if strings.TrimSpace(raw) == "" {
		return overrides, nil
	}
	var entries map[string]json.RawMessage
	if err := json.Unmarshal([]byte(raw), &entries); err != nil {
		return nil, fmt.Errorf("parse %s: %w", admissionOverridesEnv, err)
	}
	for tenant, entry := range entries {
		limits := defaultAdmissionLimits()
		if err := json.Unmarshal(entry, &limits); err != nil {
			return nil, fmt.Errorf("parse %s for tenant %q: %w", admissionOverridesEnv, tenant, err)
		}
		if err := limits.validate(); err != nil {
			return nil, fmt.Errorf("%s for tenant %q: %w", admissionOverridesEnv, tenant, err)
		}
		overrides[tenant] = limits
	}
	return overrides, nil
}

func newTenantAdmission(mode admissionMode, defaults admissionLimits, overrides map[string]admissionLimits) *tenantAdmission {
	if overrides == nil {
		overrides = map[string]admissionLimits{}
	}
	return &tenantAdmission{mode: mode, defaults: defaults, overrides: overrides, tenants: map[string]*tenantAdmissionState{}}
}

func (a *tenantAdmission) active() bool { return a != nil && a.mode != admissionOff }

// enforcing is the only mode in which admission may change a response.
func (a *tenantAdmission) enforcing() bool { return a != nil && a.mode == admissionEnforce }

func (a *tenantAdmission) limitsFor(tenant string) (admissionLimits, bool) {
	if limits, ok := a.overrides[tenant]; ok {
		return limits, true
	}
	return a.defaults, false
}

func newAdmissionState(limits admissionLimits, now time.Time) *tenantAdmissionState {
	return &tenantAdmissionState{
		limits:      limits,
		data:        newTokenBucket(limits.DataBurst, limits.DataPerSecond, now),
		meta:        newTokenBucket(limits.MetaBurst, limits.MetaPerSecond, now),
		write:       newTokenBucket(limits.WriteBurst, limits.WritePerSecond, now),
		bytes:       newTokenBucket(limits.BytesBurst, limits.BytesPerSecond, now),
		lastSeen:    now,
		wouldRefuse: map[string]uint64{},
	}
}

func (a *tenantAdmission) stateLocked(tenant string, now time.Time) *tenantAdmissionState {
	if state, ok := a.tenants[tenant]; ok {
		state.lastSeen = now
		return state
	}
	if len(a.tenants) >= maxTrackedTenants {
		for key, state := range a.tenants {
			if state.inFlight == 0 && now.Sub(state.lastSeen) > idleTenantRetention {
				delete(a.tenants, key)
			}
		}
	}
	limits, _ := a.limitsFor(tenant)
	state := newAdmissionState(limits, now)
	a.tenants[tenant] = state
	return state
}

// admit runs before any ledger call. occupied and capacity describe the global
// response pool at the moment of the decision. Write-lane admissions hold no
// in-flight slot and return a nil ticket.
func (a *tenantAdmission) admit(tenant, lane string, occupied, capacity int, now time.Time) (*admissionTicket, *admissionRefusal, http.Header) {
	if !a.active() {
		return nil, nil, nil
	}
	a.mu.Lock()
	defer a.mu.Unlock()
	state := a.stateLocked(tenant, now)
	state.refill(now)
	refusal := state.evaluate(lane, occupied, capacity)
	if refusal != nil {
		state.wouldRefuse[refusal.scope]++
		admissionWouldRefuse.WithLabelValues(refusal.lane, refusal.scope, tenantLabel(tenant)).Inc()
		if a.mode == admissionEnforce {
			admissionRefused.WithLabelValues(refusal.lane, refusal.scope, tenantLabel(tenant)).Inc()
			return nil, refusal, state.headers(lane)
		}
	}
	bucket := state.bucket(lane)
	took := bucket.tokens >= 1
	if took {
		bucket.tokens--
	}
	if lane == admissionLaneWrite {
		return nil, nil, state.headers(lane)
	}
	state.inFlight++
	return &admissionTicket{admission: a, tenant: tenant, lane: lane, tookToken: took}, nil, state.headers(lane)
}

func (s *tenantAdmissionState) evaluate(lane string, occupied, capacity int) *admissionRefusal {
	tenantRefusal := func(lane string, deficit, rate float64) *admissionRefusal {
		return &admissionRefusal{status: http.StatusTooManyRequests, scope: admissionScopeTenant, lane: lane, retryAfter: retryAfterSeconds(deficit, rate)}
	}
	bucket := s.bucket(lane)
	if lane == admissionLaneWrite {
		if bucket.tokens < 1 {
			return tenantRefusal(lane, bucket.deficit(1), bucket.rate)
		}
		return nil
	}
	if s.inFlight >= s.limits.InFlight {
		return tenantRefusal(lane, 1, 1)
	}
	if bucket.tokens < 1 {
		return tenantRefusal(lane, bucket.deficit(1), bucket.rate)
	}
	if lane == admissionLaneData && s.meta.tokens < 0 {
		return tenantRefusal(admissionLaneMeta, s.meta.deficit(0), s.meta.rate)
	}
	if lane == admissionLaneData && s.bytes.tokens <= 0 {
		return tenantRefusal(lane, s.bytes.deficit(1), s.bytes.rate)
	}
	if occupied >= fairShareOccupancy && s.inFlight > fairShareSlots(capacity) {
		return &admissionRefusal{status: http.StatusTooManyRequests, scope: admissionScopeFairShare, lane: lane, retryAfter: retryAfterSeconds(1, 1)}
	}
	return nil
}

func fairShareSlots(capacity int) int { return max(fairShareMinimum, capacity/4) }

// release returns the in-flight slot and charges the delivered body bytes to
// the byte bucket whatever the lane. A data-lane request the server answered
// 304 moves its token from the data bucket to the meta bucket.
func (t *admissionTicket) release(bytes uint64, status int) {
	if t == nil {
		return
	}
	a := t.admission
	a.mu.Lock()
	defer a.mu.Unlock()
	state, ok := a.tenants[t.tenant]
	if !ok {
		return
	}
	state.refill(time.Now())
	state.bytes.add(-float64(bytes))
	if t.lane == admissionLaneData && status == http.StatusNotModified {
		if t.tookToken {
			state.data.add(1)
		}
		state.meta.add(-1)
	}
	state.inFlight = max(0, state.inFlight-1)
}

func (s *tenantAdmissionState) headers(lane string) http.Header {
	bucket := *s.bucket(lane)
	window := func(b tokenBucket) int { return int(math.Ceil(b.capacity / b.rate)) }
	reset := func(b tokenBucket) int { return int(math.Ceil(math.Max(0, b.capacity-b.tokens) / b.rate)) }
	remaining := func(b tokenBucket) int64 { return int64(math.Max(0, math.Floor(b.tokens))) }
	header := http.Header{}
	header.Set("RateLimit-Policy", fmt.Sprintf(`%q;q=%d;w=%d, "bytes";q=%d;w=%d`,
		lane, int64(bucket.capacity), window(bucket), int64(s.bytes.capacity), window(s.bytes)))
	header.Set("RateLimit", fmt.Sprintf(`%q;r=%d;t=%d, "bytes";r=%d;t=%d`,
		lane, remaining(bucket), reset(bucket), remaining(s.bytes), reset(s.bytes)))
	return header
}

// retryAfterSeconds is the time for rate to cover deficit, plus jitter, in 1..60.
func retryAfterSeconds(deficit, rate float64) int {
	seconds := maxRetryAfter
	if rate > 0 && deficit >= 0 && !math.IsInf(deficit/rate, 0) {
		seconds = int(math.Min(maxRetryAfter, math.Ceil(deficit/rate)))
	}
	return min(maxRetryAfter, max(1, seconds+admissionJitter()))
}

func readMethod(method string) bool { return method == http.MethodGet || method == http.MethodHead }

// metadataRoutes holds the registered "METHOD template" pairs that return no
// tenant data. Matching on the template, never on the raw path, keeps
// tenant-chosen path segments (a read-model name, a prime path) out of it.
var metadataRoutes = map[string]bool{}

// admissionLane classifies by method and matched route template only; request
// headers and query parameters are caller-chosen and never select a cheaper
// lane. A write that the guard bills, because it returns tenant data, is
// charged as a data read.
func admissionLane(r *http.Request, route string) string {
	if !readMethod(r.Method) {
		if billableEgressRequest(r) {
			return admissionLaneData
		}
		return admissionLaneWrite
	}
	if route != "" && metadataRoutes[r.Method+" "+route] {
		return admissionLaneMeta
	}
	return admissionLaneData
}

var admissionRefusalErrors = map[string]string{
	admissionScopeTenant:    "tenant_rate_limited",
	admissionScopeFairShare: "global_fair_share_exceeded",
}

func writeAdmissionRefusal(w http.ResponseWriter, refusal *admissionRefusal) map[string]any {
	w.Header().Set("Retry-After", strconv.Itoa(refusal.retryAfter))
	return map[string]any{
		"error":       admissionRefusalErrors[refusal.scope],
		"scope":       refusal.scope,
		"lane":        refusal.lane,
		"retry_after": refusal.retryAfter,
		"message":     "Retry after the Retry-After interval; GET /api/v1/tenants/me/limits shows this tenant's limits.",
	}
}
