package main

// Event-sourced heartbeats — the canonical status signal for the fleet.
//
// Control Plane is the single natural host for this because (a) it already
// has internal-network access to every backend, (b) it already writes events
// to Core, and (c) its own uptime is a superset of the status page's
// usefulness — if CP is down, nothing else can report anyway.
//
// Every tick probes every backend and updates the in-memory gauge. A
// `service.heartbeat` event reaches Core under tenant=system only when a
// service's status changes, when the process starts, or when a service has
// gone heartbeatKeepaliveInterval without a write. The status endpoint folds
// those events into a current-view projection.

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"log"
	"net/http"
	"sort"
	"sync"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/infrastructure/clients"
)

// heartbeatInterval is how often we probe each service. It sets the
// in-memory gauge's resolution, not the write rate to Core.
const heartbeatInterval = 10 * time.Second

// heartbeatTTL is how long a single heartbeat counts as "fresh" when the
// status endpoint aggregates. Slightly larger than 2× the interval so one
// dropped probe doesn't flip a service red.
const heartbeatTTL = 25 * time.Second

// heartbeatKeepaliveInterval bounds how long an unchanged service may go
// without a written event, so "nothing changed" stays distinguishable from
// "nothing reported". Ticks in between write nothing.
const heartbeatKeepaliveInterval = time.Hour

// heartbeatTenant is where service heartbeats live. Not user data; a
// dedicated system tenant keeps it out of per-user queries and lets us
// set retention/compaction independently later.
const heartbeatTenant = "system"

// Status strings used across probe results, the projection, and the HTML
// status page. `statusStale` is computed at projection time, not emitted.
const (
	statusHealthy   = "healthy"
	statusUnhealthy = "unhealthy"
	statusStale     = "stale"
)

// heartbeatEventType is the event type name. Consumers (status page,
// alerting, graphs) filter on this.
const heartbeatEventType = "service.heartbeat"

// probe describes one backend the Control Plane checks. Method defaults to
// GET; bodyMatch (optional) is a substring the response body must contain
// to be considered healthy. HEAD is avoided entirely — some backends (Gin
// with default setup) don't handle HEAD for paths that only registered GET,
// which produced false-dead signals before this system replaced Vigil.
type probe struct {
	name      string
	url       string
	bodyMatch string
}

// probesFromEnv builds the probe list. Targets default to Fly internal
// hostnames for the services on the internal network, and public URLs for
// services that are publicly reachable (auth, web).
//
// All backend URLs honor env overrides to keep this configurable without
// a rebuild: CORE_HEALTH_URL, QUERY_HEALTH_URL, PRIME_HEALTH_URL,
// AUTH_HEALTH_URL, REGISTRY_HEALTH_URL, WEB_HEALTH_URL.
func probesFromEnv(getEnv func(string) string) []probe {
	probes := []probe{
		// Control Plane probes its own loopback so a stuck process still
		// emits "unhealthy" before it misses too many intervals. We hit /livez
		// (liveness only) instead of /health so the self-probe doesn't flip
		// red just because Core is down — Core has its own probe entry.
		{
			name:      "control-plane",
			url:       "http://127.0.0.1:" + DefaultPort + "/livez",
			bodyMatch: `"status":"ok"`,
		},
	}

	add := func(name, envVar, fallback, bodyMatch string) {
		u := getEnv(envVar)
		if u == "" {
			u = fallback
		}
		probes = append(probes, probe{name: name, url: u, bodyMatch: bodyMatch})
	}

	// Internal .fly network — the delegation-layer backends.
	add("core", "CORE_HEALTH_URL", "http://allsource-core.internal:3900/health", `"status":"healthy"`)
	add("query", "QUERY_HEALTH_URL", "http://allsource-query.internal:3902/health", "")
	add("prime", "PRIME_HEALTH_URL", "http://allsource-prime.internal:3905/health", `"status":"ok"`)

	// Publicly-reachable services (keep probing over public DNS so we catch
	// cert/edge issues that internal routing would hide).
	add("auth", "AUTH_HEALTH_URL", "https://allsource-auth.fly.dev/health", "")
	add("web", "WEB_HEALTH_URL", "https://all-source.xyz/", "")

	return probes
}

// heartbeatEmitter owns the probe loop + Core client. It also keeps the
// most recent probe result per service in memory so the status page can
// render without round-tripping through Core — when Core is the thing
// that's down, asking Core "is Core down?" is a circular dependency that
// turns the status page itself into a casualty (issue #160).
type heartbeatEmitter struct {
	probes     []probe
	coreClient clients.CoreClient
	httpClient *http.Client

	mu          sync.RWMutex
	latest      map[string]heartbeatSample // keyed by service name
	lastWritten map[string]writtenBeat     // keyed by service name
}

// writtenBeat is the last heartbeat that actually reached Core for a service.
// It gates writes; the gauge in `latest` is updated on every probe regardless.
type writtenBeat struct {
	status string
	at     time.Time
}

// heartbeatSample is the live, in-process record of one probe outcome.
// Stored locally so the status projection survives Core outages.
type heartbeatSample struct {
	status    string
	latencyMs int64
	probedURL string
	errMsg    string
	at        time.Time
}

func newHeartbeatEmitter(coreClient clients.CoreClient, httpClient *http.Client, probes []probe) *heartbeatEmitter {
	if httpClient == nil {
		httpClient = &http.Client{Timeout: 4 * time.Second}
	}
	return &heartbeatEmitter{
		probes:      probes,
		coreClient:  coreClient,
		httpClient:  httpClient,
		latest:      make(map[string]heartbeatSample, len(probes)),
		lastWritten: make(map[string]writtenBeat, len(probes)),
	}
}

// shouldWriteBeat reports whether this probe result needs to reach Core. A
// status change is the event; an unchanged status is re-stated only once per
// heartbeatKeepaliveInterval.
func (h *heartbeatEmitter) shouldWriteBeat(name, status string, now time.Time) bool {
	h.mu.RLock()
	defer h.mu.RUnlock()
	prev, ok := h.lastWritten[name]
	if !ok {
		return true
	}
	return prev.status != status || now.Sub(prev.at) >= heartbeatKeepaliveInterval
}

// markBeatWritten records a successful write. A failed write leaves the
// previous mark alone so the next tick retries, rather than the service going
// silent until its next transition.
func (h *heartbeatEmitter) markBeatWritten(name, status string, at time.Time) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.lastWritten[name] = writtenBeat{status: status, at: at}
}

// snapshot returns the current in-memory probe state as serviceStatus
// entries. Anything older than heartbeatTTL flips to "stale" — same rule
// as the Core-backed projection, kept consistent so callers see one shape.
func (h *heartbeatEmitter) snapshot(now time.Time) []serviceStatus {
	h.mu.RLock()
	defer h.mu.RUnlock()

	out := make([]serviceStatus, 0, len(h.latest))
	for name, s := range h.latest {
		age := now.Sub(s.at)
		entry := serviceStatus{
			Service:    name,
			LatencyMs:  s.latencyMs,
			LastSeen:   s.at.UTC().Format(time.RFC3339Nano),
			AgeSeconds: age.Seconds(),
			Error:      s.errMsg,
			ProbedURL:  s.probedURL,
		}
		if age > heartbeatTTL {
			entry.Status = statusStale
		} else {
			entry.Status = s.status
		}
		out = append(out, entry)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Service < out[j].Service })
	return out
}

func (h *heartbeatEmitter) record(name string, s heartbeatSample) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.latest[name] = s
}

// run ticks forever (until ctx cancels), emitting one heartbeat event per
// probe per interval. Probes run concurrently so a slow backend doesn't
// starve the others.
func (h *heartbeatEmitter) run(ctx context.Context, interval time.Duration) {
	// Fire once immediately so the status page has data on startup.
	h.emitAll(ctx)

	t := time.NewTicker(interval)
	defer t.Stop()
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
			h.emitAll(ctx)
		}
	}
}

func (h *heartbeatEmitter) emitAll(ctx context.Context) {
	var wg sync.WaitGroup
	for _, p := range h.probes {
		wg.Add(1)
		go func(p probe) {
			defer wg.Done()
			h.probeAndEmit(ctx, p)
		}(p)
	}
	wg.Wait()
}

// probeAndEmit runs one HTTP probe, records the result in the in-process
// cache (always — that's what powers the status page during Core outages),
// then best-effort writes it to Core when shouldWriteBeat allows. Both
// healthy and unhealthy results are written, so consumers can distinguish
// "last seen healthy" from "probe never ran".
func (h *heartbeatEmitter) probeAndEmit(ctx context.Context, p probe) {
	started := time.Now()
	status, errMsg := h.probeOnce(ctx, p)
	latency := time.Since(started)

	h.record(p.name, heartbeatSample{
		status:    status,
		latencyMs: latency.Milliseconds(),
		probedURL: p.url,
		errMsg:    errMsg,
		at:        started,
	})

	if !h.shouldWriteBeat(p.name, status, started) {
		return
	}

	payload := map[string]any{
		"service":    p.name,
		"status":     status,
		"latency_ms": latency.Milliseconds(),
		"probed_url": p.url,
	}
	if errMsg != "" {
		payload["error"] = errMsg
	}

	// Core is for history. If Core is down the in-memory cache above keeps
	// the status page working — this write is best-effort.
	if _, err := h.coreClient.IngestEvent(ctx, clients.IngestEventRequest{
		EventType: heartbeatEventType,
		EntityID:  p.name,
		TenantID:  heartbeatTenant,
		Payload:   payload,
	}); err != nil {
		log.Printf("heartbeat: ingest failed for %s: %v", p.name, err)
		return
	}
	h.markBeatWritten(p.name, status, started)
}

func (h *heartbeatEmitter) probeOnce(ctx context.Context, p probe) (status, errMsg string) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, p.url, http.NoBody)
	if err != nil {
		return statusUnhealthy, "build request: " + err.Error()
	}
	resp, err := h.httpClient.Do(req)
	if err != nil {
		return statusUnhealthy, "request: " + err.Error()
	}
	defer func() {
		if closeErr := resp.Body.Close(); closeErr != nil {
			log.Printf("heartbeat: close response body for %s: %v", p.name, closeErr)
		}
	}()

	if resp.StatusCode < 200 || resp.StatusCode >= 400 {
		return statusUnhealthy, fmt.Sprintf("http %d", resp.StatusCode)
	}
	if p.bodyMatch != "" {
		body, err := io.ReadAll(io.LimitReader(resp.Body, 8192))
		if err != nil {
			return statusUnhealthy, "read body: " + err.Error()
		}
		if !bytes.Contains(body, []byte(p.bodyMatch)) {
			return statusUnhealthy, "body missing expected marker"
		}
	}
	return statusHealthy, ""
}

// --------------- Status projection ---------------

// serviceStatus is the current-view shape returned by /api/v1/status/services.
type serviceStatus struct {
	Service    string  `json:"service"`
	Status     string  `json:"status"`      // "healthy" | "unhealthy" | "stale"
	LatencyMs  int64   `json:"latency_ms"`  // from the last heartbeat
	LastSeen   string  `json:"last_seen"`   // RFC3339 of last heartbeat received
	AgeSeconds float64 `json:"age_seconds"` // seconds since last heartbeat
	Error      string  `json:"error,omitempty"`
	ProbedURL  string  `json:"probed_url,omitempty"`
}

// projectStatus folds the recent heartbeat events into one entry per
// service, picking the newest heartbeat and flagging anything older than
// heartbeatTTL as "stale".
func projectStatus(events []clients.EventEntry, now time.Time) []serviceStatus {
	// newest heartbeat per entity_id
	latest := map[string]clients.EventEntry{}
	for _, e := range events {
		if e.EventType != heartbeatEventType {
			continue
		}
		if prev, ok := latest[e.EntityID]; ok {
			if e.Timestamp <= prev.Timestamp {
				continue
			}
		}
		latest[e.EntityID] = e
	}

	out := make([]serviceStatus, 0, len(latest))
	for _, e := range latest {
		s := serviceStatus{Service: e.EntityID, LastSeen: e.Timestamp}
		ts, err := time.Parse(time.RFC3339Nano, e.Timestamp)
		if err != nil {
			var fallbackErr error
			ts, fallbackErr = time.Parse(time.RFC3339, e.Timestamp)
			if fallbackErr != nil {
				log.Printf("heartbeat: unparseable timestamp %q for %s: %v", e.Timestamp, e.EntityID, fallbackErr)
			}
		}
		age := now.Sub(ts)
		s.AgeSeconds = age.Seconds()

		var reported string
		if v, ok := e.Payload["status"].(string); ok {
			reported = v
		}
		switch {
		case age > heartbeatTTL:
			s.Status = statusStale
		default:
			s.Status = reported
		}
		if v, ok := e.Payload["latency_ms"].(float64); ok {
			s.LatencyMs = int64(v)
		}
		if v, ok := e.Payload["error"].(string); ok {
			s.Error = v
		}
		if v, ok := e.Payload["probed_url"].(string); ok {
			s.ProbedURL = v
		}
		out = append(out, s)
	}

	sort.Slice(out, func(i, j int) bool { return out[i].Service < out[j].Service })
	return out
}

// statusServicesHandler returns the JSON projection. The marketing site's
// /status page (www.all-source.xyz/status) is the canonical user-facing UI;
// this is the stable JSON feed it polls.
func (cp *ControlPlane) statusServicesHandler(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"services": cp.currentServiceStatus()})
}

// currentServiceStatus returns the live projection. Reads first from the
// in-process probe cache (always available — survives Core outages) and
// falls back to merging in older heartbeats from Core if the cache is
// cold (e.g. CP just started). The cache is the load-bearing path; Core
// is for history and external consumers, not for rendering "is X up right
// now?" — that would be a circular dependency on Core itself.
func (cp *ControlPlane) currentServiceStatus() []serviceStatus {
	if cp.heartbeats != nil {
		if live := cp.heartbeats.snapshot(time.Now()); len(live) > 0 {
			return live
		}
	}

	// Cold cache (CP just booted): try Core for any recent heartbeats so
	// the page isn't blank during startup. Tight 1s timeout so a slow Core
	// can't make the status page hang.
	ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
	defer cancel()

	// The window must exceed heartbeatKeepaliveInterval, or a steady service
	// that has not changed status since its last keepalive falls outside it
	// and the page renders blank. projectStatus still ages these to "stale",
	// which is the honest reading of a beat this old.
	since := time.Now().Add(-2 * heartbeatKeepaliveInterval).UTC().Format(time.RFC3339)
	resp, err := cp.coreClient.QueryEvents(ctx, clients.QueryEventsRequest{
		EventType: heartbeatEventType,
		TenantID:  heartbeatTenant,
		Since:     since,
		Limit:     500,
	})
	if err != nil || resp == nil {
		return nil
	}
	return projectStatus(resp.Events, time.Now())
}
