package main

import (
	"fmt"
	"log"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/prometheus/client_golang/prometheus"
)

const legacyKeySunsetEnv = "ALLSOURCE_LEGACY_KEY_SUNSET"

// legacyKeyDeprecatedAt is the day registered ES256 client keys replaced
// unregistered managed keys (docs/decisions/2026-11-07-tenant-clients-registered-es256-keys.md).
var legacyKeyDeprecatedAt = time.Date(2026, 11, 7, 0, 0, 0, 0, time.UTC)

var legacyKeyRequests = tenantMetrics.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_legacy_key_requests_total",
	Help: "Requests served with an unregistered API key before ALLSOURCE_LEGACY_KEY_SUNSET.",
}, []string{"tenant"})

func parseLegacyKeySunset(raw string) (time.Time, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return time.Time{}, nil
	}
	at, err := time.Parse(time.RFC3339, raw)
	if err != nil {
		return time.Time{}, fmt.Errorf("%s must be an RFC3339 instant, got %q", legacyKeySunsetEnv, raw)
	}
	return at.UTC(), nil
}

// legacySunset decides the fate of a credential the key authority classes as
// unregistered. Unset, it changes nothing; set, shadow and enforce both serve
// such a credential until the instant and refuse it afterwards. Off never asks.
type legacySunset struct {
	at  time.Time
	now func() time.Time

	mu         sync.Mutex
	lastLogged map[string]time.Time
}

func newLegacySunset(at time.Time) *legacySunset {
	return &legacySunset{at: at, now: time.Now, lastLogged: map[string]time.Time{}}
}

func (l *legacySunset) configured() bool {
	return l != nil && !l.at.IsZero()
}

func (l *legacySunset) serve(tenant string) bool {
	now := l.now()
	if !now.Before(l.at) {
		return false
	}
	legacyKeyRequests.WithLabelValues(tenantLabel(tenant)).Inc()
	l.mu.Lock()
	last, seen := l.lastLogged[tenant]
	if seen && now.Sub(last) < denialLogInterval {
		l.mu.Unlock()
		return true
	}
	if len(l.lastLogged) >= keyAuthorityCacheLimit {
		l.lastLogged = map[string]time.Time{}
	}
	l.lastLogged[tenant] = now
	l.mu.Unlock()
	log.Printf("legacy API key served: tenant=%s sunset=%s", tenant, l.at.Format(time.RFC3339)) //nolint:gosec // G706: tenant comes from a verified signature.
	return true
}

// announce marks a response served with a legacy key: Deprecation per RFC 9745
// and Sunset per RFC 8594, so a client can see the cut-off before it lands.
func (l *legacySunset) announce(c *gin.Context) {
	c.Header("Deprecation", "@"+strconv.FormatInt(legacyKeyDeprecatedAt.Unix(), 10))
	c.Header("Sunset", l.at.UTC().Format(http.TimeFormat))
}

func (l *legacySunset) refusal() gin.H {
	return gin.H{
		"error":   "unauthorized",
		"reason":  "legacy_key_sunset",
		"message": "This API key predates registered keys and stopped working on " + l.at.Format("2006-01-02") + ". Register a client key or create a dashboard key.",
	}
}
