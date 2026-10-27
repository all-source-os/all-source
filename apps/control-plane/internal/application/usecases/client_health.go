package usecases

// Client health for the admin /monitoring page: are clients being served right
// now, and who is being refused? Read from Fly managed Prometheus, which keeps
// the control plane's per-request counters across restarts and deploys.

import (
	"context"
	"errors"
	"fmt"
	"math"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// ErrInvalidHealthRange is returned for a range other than 1h, 24h or 7d.
var ErrInvalidHealthRange = errors.New("range must be one of 1h, 24h, 7d")

// PromQuerier is the read side of the Prometheus HTTP API.
type PromQuerier interface {
	Query(ctx context.Context, query string, at time.Time) ([]clients.PromSample, error)
	QueryRange(ctx context.Context, query string, start, end time.Time, step time.Duration) ([]clients.PromSeries, error)
}

const (
	clientHealthNowWindow  = 5 * time.Minute
	clientHealthQueryLimit = 5 * time.Second
	clientHealthCacheTTL   = 15 * time.Second
	releaseMergeWindow     = 60 * time.Second

	quietMinBaselinePerMin = 10.0
	quietServedShare       = 0.05

	anonymousTenantLabel = "anonymous"

	clientDataRoutes = `/api/v1/events/query|/api/v1/event-feed|/api/v1/events|/api/v1/read-models/:name|/api/v1/stats`
	healthApps       = `allsource-control-plane|allsource-core|allsource-query`

	cpRequestsSelector     = `control_plane_http_requests_total{app="allsource-control-plane",path=~"` + clientDataRoutes + `"}`
	tenantRequestsSelector = `control_plane_tenant_requests_total{app="allsource-control-plane",route=~"` + clientDataRoutes + `"}`
	uptimeSelector         = `fly_instance_uptime_seconds{app=~"` + healthApps + `"}`
)

// Tenant states in a client-health report.
const (
	TenantStateServed  = "served"
	TenantStateQuiet   = "quiet"
	TenantStateRefused = "refused"
	TenantStateIdle    = "idle"
)

// PromQLStatusWindow sums client data requests by HTTP status over window.
func PromQLStatusWindow(window time.Duration) string {
	return fmt.Sprintf(`sum by (status) (increase(%s[%ds]))`, cpRequestsSelector, int64(window/time.Second))
}

// Client-health PromQL, exported so a report can be reproduced by hand. The
// tenant window matches the `now` window so a fresh outage is not hidden by
// the traffic served before it. The baseline spans 24h so it outlasts a
// multi-hour outage.
const (
	PromQLTenantsRecent  = `sum by (tenant, status_class) (increase(` + tenantRequestsSelector + `[5m]))`
	PromQLTenantBaseline = `sum by (tenant) (increase(` + tenantRequestsSelector + `[24h] offset 5m)) / 1440`
	PromQLStartTimes     = `max by (app, instance) (timestamp(` + uptimeSelector + `) - ` + uptimeSelector + `)`
	PromQLMachineUp      = `max by (app, instance) (fly_instance_up{app=~"` + healthApps + `"})`
	PromQLUptime         = `max by (app, instance) (` + uptimeSelector + `)`
)

type healthRange struct {
	span time.Duration
	step time.Duration
}

var healthRanges = map[string]healthRange{
	"1h":  {span: time.Hour, step: time.Minute},
	"24h": {span: 24 * time.Hour, step: 5 * time.Minute},
	"7d":  {span: 7 * 24 * time.Hour, step: time.Hour},
}

// ClientHealthNow is the status mix of client data requests in the last window.
type ClientHealthNow struct {
	WindowSeconds int      `json:"window_seconds"`
	Served        int64    `json:"served"`
	RefusedAuth   int64    `json:"refused_auth"`
	Throttled     int64    `json:"throttled"`
	Failed        int64    `json:"failed"`
	Total         int64    `json:"total"`
	ServedRatio   *float64 `json:"served_ratio"`
}

// ClientHealthPoint is one step of the client data request series.
type ClientHealthPoint struct {
	T           string `json:"t"`
	Served      int64  `json:"served"`
	RefusedAuth int64  `json:"refused_auth"`
	Throttled   int64  `json:"throttled"`
	Failed      int64  `json:"failed"`
}

// TenantHealth is one tenant's client data traffic. Tenant "anonymous" is
// traffic whose authentication failed. Counts cover the last 5 minutes;
// BaselinePerMin is the average rate over the 24 hours before that.
type TenantHealth struct {
	Tenant         string  `json:"tenant"`
	Served5m       int64   `json:"served_5m"`
	Refused5m      int64   `json:"refused_5m"`
	Failed5m       int64   `json:"failed_5m"`
	BaselinePerMin float64 `json:"baseline_per_min"`
	State          string  `json:"state"`
}

// Release is a machine start inside the reported range.
type Release struct {
	App       string `json:"app"`
	Instance  string `json:"instance"`
	StartedAt string `json:"started_at"`
}

// Machine is a currently reporting Fly machine.
type Machine struct {
	App           string `json:"app"`
	Instance      string `json:"instance"`
	Up            bool   `json:"up"`
	StartedAt     string `json:"started_at"`
	UptimeSeconds int64  `json:"uptime_seconds"`
}

// ClientHealthReport is the GET /api/v1/admin/metrics/client-health body.
type ClientHealthReport struct {
	Configured  bool                `json:"configured"`
	Error       string              `json:"error,omitempty"`
	Range       string              `json:"range"`
	StepSeconds int64               `json:"step_seconds,omitempty"`
	GeneratedAt string              `json:"generated_at"`
	Now         *ClientHealthNow    `json:"now,omitempty"`
	Series      []ClientHealthPoint `json:"series,omitempty"`
	Tenants     []TenantHealth      `json:"tenants,omitempty"`
	Releases    []Release           `json:"releases,omitempty"`
	Machines    []Machine           `json:"machines,omitempty"`
}

type cachedHealth struct {
	report  ClientHealthReport
	expires time.Time
}

// ClientHealthUseCase builds ClientHealthReport from Prometheus. A nil querier
// means the Prometheus token is not set.
type ClientHealthUseCase struct {
	prom     PromQuerier
	now      func() time.Time
	mu       sync.Mutex
	cache    map[string]cachedHealth
	inflight map[string]*healthCall
}

type healthCall struct {
	done   chan struct{}
	report ClientHealthReport
	err    error
}

// NewClientHealthUseCase creates the use case. prom may be nil.
func NewClientHealthUseCase(prom PromQuerier) *ClientHealthUseCase {
	return &ClientHealthUseCase{prom: prom, now: time.Now, cache: map[string]cachedHealth{}, inflight: map[string]*healthCall{}}
}

// WithClock replaces the clock (tests).
func (uc *ClientHealthUseCase) WithClock(now func() time.Time) *ClientHealthUseCase {
	uc.now = now
	return uc
}

// Report returns the client-health report for rangeKey. An unconfigured use
// case returns Configured=false and no numbers; an upstream failure returns an
// error and no numbers. Concurrent cache misses for one range share a single
// upstream fetch.
func (uc *ClientHealthUseCase) Report(ctx context.Context, rangeKey string) (ClientHealthReport, error) {
	hr, ok := healthRanges[rangeKey]
	if !ok {
		return ClientHealthReport{}, ErrInvalidHealthRange
	}
	now := uc.now().UTC()
	if uc.prom == nil {
		return ClientHealthReport{Configured: false, Range: rangeKey, GeneratedAt: now.Format(time.RFC3339)}, nil
	}

	uc.mu.Lock()
	if c, hit := uc.cache[rangeKey]; hit && now.Before(c.expires) {
		uc.mu.Unlock()
		return c.report, nil
	}
	if call, busy := uc.inflight[rangeKey]; busy {
		uc.mu.Unlock()
		select {
		case <-call.done:
			return call.report, call.err
		case <-ctx.Done():
			return ClientHealthReport{}, ctx.Err()
		}
	}
	call := &healthCall{done: make(chan struct{})}
	uc.inflight[rangeKey] = call
	uc.mu.Unlock()

	call.report, call.err = uc.build(context.WithoutCancel(ctx), rangeKey, hr, now)
	uc.mu.Lock()
	delete(uc.inflight, rangeKey)
	if call.err == nil {
		uc.cache[rangeKey] = cachedHealth{report: call.report, expires: now.Add(clientHealthCacheTTL)}
	}
	uc.mu.Unlock()
	close(call.done)
	if call.err != nil {
		return ClientHealthReport{}, call.err
	}
	return call.report, nil
}

type healthFetch struct {
	nowMix   []clients.PromSample
	series   []clients.PromSeries
	tenants  []clients.PromSample
	baseline []clients.PromSample
	starts   []clients.PromSeries
	up       []clients.PromSample
	uptime   []clients.PromSample
}

func (uc *ClientHealthUseCase) build(ctx context.Context, rangeKey string, hr healthRange, now time.Time) (ClientHealthReport, error) {
	end := now.Truncate(hr.step)
	start := end.Add(-hr.span + hr.step)

	var (
		f        healthFetch
		wg       sync.WaitGroup
		errMu    sync.Mutex
		firstErr error
	)
	run := func(name string, fn func(context.Context) error) {
		wg.Add(1)
		go func() {
			defer wg.Done()
			qctx, cancel := context.WithTimeout(ctx, clientHealthQueryLimit)
			defer cancel()
			if err := fn(qctx); err != nil {
				errMu.Lock()
				if firstErr == nil {
					firstErr = fmt.Errorf("%s: %w", name, err)
				}
				errMu.Unlock()
			}
		}()
	}
	instant := func(name, q string, dst *[]clients.PromSample) {
		run(name, func(c context.Context) (err error) {
			*dst, err = uc.prom.Query(c, q, now)
			return err
		})
	}
	ranged := func(name, q string, dst *[]clients.PromSeries) {
		run(name, func(c context.Context) (err error) {
			*dst, err = uc.prom.QueryRange(c, q, start, end, hr.step)
			return err
		})
	}

	instant("requests now", PromQLStatusWindow(clientHealthNowWindow), &f.nowMix)
	ranged("requests series", PromQLStatusWindow(hr.step), &f.series)
	instant("tenants 5m", PromQLTenantsRecent, &f.tenants)
	instant("tenant baseline", PromQLTenantBaseline, &f.baseline)
	ranged("machine starts", PromQLStartTimes, &f.starts)
	instant("machines up", PromQLMachineUp, &f.up)
	instant("machine uptime", PromQLUptime, &f.uptime)
	wg.Wait()
	if firstErr != nil {
		return ClientHealthReport{}, firstErr
	}

	return ClientHealthReport{
		Configured:  true,
		Range:       rangeKey,
		StepSeconds: int64(hr.step / time.Second),
		GeneratedAt: now.Format(time.RFC3339),
		Now:         buildNow(f.nowMix),
		Series:      buildSeries(f.series),
		Tenants:     buildTenants(f.tenants, f.baseline),
		Releases:    buildReleases(f.starts, now.Add(-hr.span)),
		Machines:    buildMachines(f.up, f.uptime, now),
	}, nil
}

type statusMix struct {
	served, refusedAuth, throttled, failed, total float64
}

func (m *statusMix) add(status string, v float64) {
	m.total += v
	switch {
	case strings.HasPrefix(status, "2"):
		m.served += v
	case status == "401" || status == "403":
		m.refusedAuth += v
	case status == "429":
		m.throttled += v
	case strings.HasPrefix(status, "5"):
		m.failed += v
	}
}

func count(v float64) int64 {
	if math.IsNaN(v) || math.IsInf(v, 0) || v < 0 {
		return 0
	}
	return int64(math.Round(v))
}

func buildNow(samples []clients.PromSample) *ClientHealthNow {
	var m statusMix
	for _, s := range samples {
		m.add(s.Labels["status"], s.Value)
	}
	out := &ClientHealthNow{
		WindowSeconds: int(clientHealthNowWindow / time.Second),
		Served:        count(m.served),
		RefusedAuth:   count(m.refusedAuth),
		Throttled:     count(m.throttled),
		Failed:        count(m.failed),
		Total:         count(m.total),
	}
	if out.Total > 0 {
		ratio := float64(out.Served) / float64(out.Total)
		out.ServedRatio = &ratio
	}
	return out
}

func buildSeries(series []clients.PromSeries) []ClientHealthPoint {
	byTime := map[int64]*statusMix{}
	for _, s := range series {
		status := s.Labels["status"]
		for _, p := range s.Points {
			k := p.Time.Unix()
			if byTime[k] == nil {
				byTime[k] = &statusMix{}
			}
			byTime[k].add(status, p.Value)
		}
	}
	keys := make([]int64, 0, len(byTime))
	for k := range byTime {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool { return keys[i] < keys[j] })
	out := make([]ClientHealthPoint, 0, len(keys))
	for _, k := range keys {
		m := byTime[k]
		out = append(out, ClientHealthPoint{
			T:           time.Unix(k, 0).UTC().Format(time.RFC3339),
			Served:      count(m.served),
			RefusedAuth: count(m.refusedAuth),
			Throttled:   count(m.throttled),
			Failed:      count(m.failed),
		})
	}
	return out
}

// TenantState classifies a tenant's last 5 minutes against its 24h baseline.
func TenantState(served, refused, failed int64, baselinePerMin float64) string {
	switch {
	case refused > 0 && refused > served+failed:
		return TenantStateRefused
	case baselinePerMin >= quietMinBaselinePerMin && float64(served) < math.Max(1, quietServedShare*baselinePerMin*clientHealthNowWindow.Minutes()):
		return TenantStateQuiet
	case served > 0:
		return TenantStateServed
	default:
		return TenantStateIdle
	}
}

var tenantStateRank = map[string]int{TenantStateQuiet: 0, TenantStateRefused: 1, TenantStateServed: 2, TenantStateIdle: 3}

func buildTenants(recent, baseline []clients.PromSample) []TenantHealth {
	type acc struct {
		served, refused, failed, baseline float64
	}
	byTenant := map[string]*acc{}
	get := func(t string) *acc {
		if byTenant[t] == nil {
			byTenant[t] = &acc{}
		}
		return byTenant[t]
	}
	for _, s := range recent {
		a := get(s.Labels["tenant"])
		switch s.Labels["status_class"] {
		case "2xx":
			a.served += s.Value
		case "4xx":
			a.refused += s.Value
		case "5xx":
			a.failed += s.Value
		}
	}
	for _, s := range baseline {
		get(s.Labels["tenant"]).baseline += s.Value
	}

	out := make([]TenantHealth, 0, len(byTenant))
	for tenant, a := range byTenant {
		served, refused, failed := count(a.served), count(a.refused), count(a.failed)
		base := math.Round(a.baseline*10) / 10
		if served == 0 && refused == 0 && failed == 0 && base == 0 {
			continue
		}
		state := TenantState(served, refused, failed, base)
		if tenant == anonymousTenantLabel {
			// Failed-auth traffic is never served, so it cannot go quiet.
			state = TenantStateIdle
			if refused > 0 {
				state = TenantStateRefused
			}
		}
		out = append(out, TenantHealth{
			Tenant:         tenant,
			Served5m:       served,
			Refused5m:      refused,
			Failed5m:       failed,
			BaselinePerMin: base,
			State:          state,
		})
	}
	sort.Slice(out, func(i, j int) bool {
		if ri, rj := tenantStateRank[out[i].State], tenantStateRank[out[j].State]; ri != rj {
			return ri < rj
		}
		if out[i].Served5m != out[j].Served5m {
			return out[i].Served5m > out[j].Served5m
		}
		return out[i].Tenant < out[j].Tenant
	})
	return out
}

func buildReleases(starts []clients.PromSeries, rangeStart time.Time) []Release {
	out := []Release{}
	for _, s := range starts {
		var last time.Time
		for _, p := range s.Points {
			st := time.Unix(int64(math.Round(p.Value)), 0).UTC()
			if !last.IsZero() && absDuration(st.Sub(last)) < releaseMergeWindow {
				continue
			}
			last = st
			if st.Before(rangeStart) {
				continue
			}
			out = append(out, Release{App: s.Labels["app"], Instance: s.Labels["instance"], StartedAt: st.Format(time.RFC3339)})
		}
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].StartedAt != out[j].StartedAt {
			return out[i].StartedAt < out[j].StartedAt
		}
		return out[i].App < out[j].App
	})
	return out
}

func buildMachines(up, uptime []clients.PromSample, now time.Time) []Machine {
	key := func(l map[string]string) string { return l["app"] + "\x00" + l["instance"] }
	byKey := map[string]*Machine{}
	for _, s := range uptime {
		secs := count(s.Value)
		byKey[key(s.Labels)] = &Machine{
			App:           s.Labels["app"],
			Instance:      s.Labels["instance"],
			UptimeSeconds: secs,
			StartedAt:     now.Add(-time.Duration(secs) * time.Second).Format(time.RFC3339),
		}
	}
	for _, s := range up {
		m := byKey[key(s.Labels)]
		if m == nil {
			m = &Machine{App: s.Labels["app"], Instance: s.Labels["instance"]}
			byKey[key(s.Labels)] = m
		}
		m.Up = s.Value >= 1
	}
	out := make([]Machine, 0, len(byKey))
	for _, m := range byKey {
		out = append(out, *m)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].App != out[j].App {
			return out[i].App < out[j].App
		}
		return out[i].Instance < out[j].Instance
	})
	return out
}

func absDuration(d time.Duration) time.Duration {
	if d < 0 {
		return -d
	}
	return d
}
