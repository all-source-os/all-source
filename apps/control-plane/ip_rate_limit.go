package main

import (
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
)

// IPRateLimiter enforces a per-IP sliding window rate limit. Designed for
// low-traffic endpoints like /onboard/start and /demo/start where you want
// to prevent bulk abuse without needing a Redis-backed solution.
//
// Thread-safe via sync.RWMutex. Stale entries are cleaned up periodically.
type IPRateLimiter struct {
	mu       sync.RWMutex
	windows  map[string][]time.Time
	limit    int
	window   time.Duration
	stopChan chan struct{}
}

// NewIPRateLimiter creates a rate limiter that allows `limit` requests per
// `window` duration per IP address. Starts a background goroutine that
// cleans up stale entries every 10 minutes.
func NewIPRateLimiter(limit int, window time.Duration) *IPRateLimiter {
	rl := &IPRateLimiter{
		windows:  make(map[string][]time.Time),
		limit:    limit,
		window:   window,
		stopChan: make(chan struct{}),
	}
	go rl.cleanup()
	return rl
}

// Allow checks whether the given IP is within the rate limit. If allowed,
// records the request and returns true. If rate-limited, returns false.
func (rl *IPRateLimiter) Allow(ip string) bool {
	rl.mu.Lock()
	defer rl.mu.Unlock()

	now := time.Now()
	cutoff := now.Add(-rl.window)

	// Filter to only requests within the window
	timestamps := rl.windows[ip]
	valid := timestamps[:0]
	for _, ts := range timestamps {
		if ts.After(cutoff) {
			valid = append(valid, ts)
		}
	}

	if len(valid) >= rl.limit {
		rl.windows[ip] = valid
		return false
	}

	rl.windows[ip] = append(valid, now)
	return true
}

// RetryAfter returns the number of seconds until the oldest request in the
// window expires, giving the client a useful Retry-After header value.
func (rl *IPRateLimiter) RetryAfter(ip string) int {
	rl.mu.RLock()
	defer rl.mu.RUnlock()

	timestamps := rl.windows[ip]
	if len(timestamps) == 0 {
		return 0
	}

	oldest := timestamps[0]
	expiresAt := oldest.Add(rl.window)
	remaining := time.Until(expiresAt)
	if remaining <= 0 {
		return 0
	}
	return int(remaining.Seconds()) + 1
}

// cleanup runs every 10 minutes to remove IPs with no recent requests.
func (rl *IPRateLimiter) cleanup() {
	ticker := time.NewTicker(10 * time.Minute)
	defer ticker.Stop()

	for {
		select {
		case <-ticker.C:
			rl.mu.Lock()
			now := time.Now()
			cutoff := now.Add(-rl.window)
			for ip, timestamps := range rl.windows {
				valid := timestamps[:0]
				for _, ts := range timestamps {
					if ts.After(cutoff) {
						valid = append(valid, ts)
					}
				}
				if len(valid) == 0 {
					delete(rl.windows, ip)
				} else {
					rl.windows[ip] = valid
				}
			}
			rl.mu.Unlock()
		case <-rl.stopChan:
			return
		}
	}
}

// Stop halts the background cleanup goroutine.
func (rl *IPRateLimiter) Stop() {
	close(rl.stopChan)
}

// IPRateLimitMiddleware returns a Gin middleware that rate-limits requests
// by client IP. Extracts IP from X-Forwarded-For (set by Fly proxy) or
// falls back to RemoteAddr.
func IPRateLimitMiddleware(limiter *IPRateLimiter) gin.HandlerFunc {
	return func(c *gin.Context) {
		ip := clientIP(c)
		if !limiter.Allow(ip) {
			retryAfter := limiter.RetryAfter(ip)
			c.Header("Retry-After", strconv.Itoa(retryAfter))
			c.JSON(http.StatusTooManyRequests, gin.H{
				"error":       "rate_limited",
				"message":     "Too many requests. Please try again later.",
				"retry_after": retryAfter,
			})
			c.Abort()
			return
		}
		c.Next()
	}
}

// clientIP extracts the client IP from the request. Fly's edge proxy sets
// Fly-Client-IP on every public request and replaces any client-supplied
// value, whereas the first X-Forwarded-For entry is whatever the client sent.
// X-Forwarded-For is only trusted when Fly-Client-IP is absent, which is the
// private-network path the web app uses to forward a visitor's address.
func clientIP(c *gin.Context) string {
	if ip := strings.TrimSpace(c.GetHeader("Fly-Client-IP")); ip != "" {
		return ip
	}
	if xff := c.GetHeader("X-Forwarded-For"); xff != "" {
		// X-Forwarded-For can be "client, proxy1, proxy2" — take the first
		parts := strings.SplitN(xff, ",", 2)
		ip := strings.TrimSpace(parts[0])
		if ip != "" {
			return ip
		}
	}
	// Fallback to Gin's built-in IP detection
	return c.ClientIP()
}
