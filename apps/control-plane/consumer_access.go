package main

import (
	"context"
	"encoding/json"
	"log"
	"net/http"
	"os"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
)

const (
	pathConsumerAccess = "/readyz/consumers"
	// consumerCanaryEnv holds a JSON array of {"kind","token"}: one credential
	// of every kind customers already hold. See docs/runbooks/CONSUMER_ACCESS_GATE.md.
	consumerCanaryEnv = "CONSUMER_CANARY_CREDENTIALS"
	// The endpoint is public, so results are reused for this long; one request
	// cannot fan out into more than one probe round per window.
	consumerAccessCacheTTL = 30 * time.Second
	consumerProbeTimeout   = 10 * time.Second
	consumerProbePath      = "/api/v1/events/query?entity_id=consumer-access-canary&limit=1"
)

type consumerCanary struct {
	Kind  string `json:"kind"`
	Token string `json:"token"`
}

type consumerKindResult struct {
	Kind   string `json:"kind"`
	Status int    `json:"status"`
	OK     bool   `json:"ok"`
}

type consumerAccessResult struct {
	OK        bool                 `json:"ok"`
	Reason    string               `json:"reason,omitempty"`
	CheckedAt time.Time            `json:"checked_at"`
	Kinds     []consumerKindResult `json:"kinds"`
}

// consumerAccessProbe sends each canary credential through this machine's own
// listener, so the whole user path (auth middleware, key lifecycle, egress
// guard, Core read) is exercised. Fly's deploy waits on the top-level check
// that calls it, so a build that rejects a credential kind customers hold never
// receives traffic.
type consumerAccessProbe struct {
	canaries []consumerCanary
	base     string
	client   *http.Client

	mu       sync.Mutex
	cached   consumerAccessResult
	cachedAt time.Time
}

func newConsumerAccessProbe() *consumerAccessProbe {
	port := os.Getenv("PORT")
	if port == "" {
		port = DefaultPort
	}
	return &consumerAccessProbe{
		canaries: parseConsumerCanaries(os.Getenv(consumerCanaryEnv)),
		base:     "http://127.0.0.1:" + port,
		client:   &http.Client{Timeout: consumerProbeTimeout},
	}
}

func parseConsumerCanaries(raw string) []consumerCanary {
	if raw == "" {
		return nil
	}
	var parsed []consumerCanary
	if err := json.Unmarshal([]byte(raw), &parsed); err != nil {
		log.Printf("consumer access: %s is not a JSON array of {kind,token}: %v", consumerCanaryEnv, err)
		return nil
	}
	canaries := parsed[:0]
	for _, canary := range parsed {
		if canary.Kind != "" && canary.Token != "" {
			canaries = append(canaries, canary)
		}
	}
	return canaries
}

func (p *consumerAccessProbe) handle(c *gin.Context) {
	result := p.result(c.Request.Context())
	status := http.StatusOK
	if !result.OK {
		status = http.StatusServiceUnavailable
	}
	c.JSON(status, result)
}

func (p *consumerAccessProbe) result(ctx context.Context) consumerAccessResult {
	p.mu.Lock()
	defer p.mu.Unlock()
	if !p.cachedAt.IsZero() && time.Since(p.cachedAt) < consumerAccessCacheTTL {
		return p.cached
	}
	p.cached = p.run(ctx)
	p.cachedAt = time.Now()
	return p.cached
}

func (p *consumerAccessProbe) run(ctx context.Context) consumerAccessResult {
	result := consumerAccessResult{CheckedAt: time.Now().UTC(), Kinds: []consumerKindResult{}}
	if len(p.canaries) == 0 {
		result.Reason = "no canary credentials configured"
		return result
	}
	result.OK = true
	for _, canary := range p.canaries {
		status := p.probe(ctx, canary.Token)
		ok := status == http.StatusOK
		if !ok {
			result.OK = false
			log.Printf("consumer access: credential kind %q answered %d", canary.Kind, status)
		}
		result.Kinds = append(result.Kinds, consumerKindResult{Kind: canary.Kind, Status: status, OK: ok})
	}
	if !result.OK {
		result.Reason = "an existing credential kind cannot read through this machine"
	}
	return result
}

// probe returns the HTTP status, or 0 when no response arrived.
func (p *consumerAccessProbe) probe(ctx context.Context, token string) int {
	ctx, cancel := context.WithTimeout(ctx, consumerProbeTimeout)
	defer cancel()
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, p.base+consumerProbePath, http.NoBody)
	if err != nil {
		return 0
	}
	request.Header.Set("Authorization", "Bearer "+token)
	response, err := p.client.Do(request)
	if err != nil {
		return 0
	}
	closeResponseBody(response)
	return response.StatusCode
}
