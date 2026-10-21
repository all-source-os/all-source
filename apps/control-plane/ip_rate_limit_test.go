package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
)

func TestIPRateLimiter_AllowsUpToLimit(t *testing.T) {
	rl := NewIPRateLimiter(5, time.Hour)
	defer rl.Stop()

	for i := 0; i < 5; i++ {
		if !rl.Allow("1.2.3.4") {
			t.Fatalf("request %d should be allowed", i+1)
		}
	}
}

func TestIPRateLimiter_BlocksOverLimit(t *testing.T) {
	rl := NewIPRateLimiter(3, time.Hour)
	defer rl.Stop()

	for i := 0; i < 3; i++ {
		rl.Allow("1.2.3.4")
	}

	if rl.Allow("1.2.3.4") {
		t.Error("4th request should be blocked")
	}
}

func TestIPRateLimiter_DifferentIPsIndependent(t *testing.T) {
	rl := NewIPRateLimiter(2, time.Hour)
	defer rl.Stop()

	rl.Allow("1.1.1.1")
	rl.Allow("1.1.1.1")

	if rl.Allow("1.1.1.1") {
		t.Error("1.1.1.1 should be blocked")
	}
	if !rl.Allow("2.2.2.2") {
		t.Error("2.2.2.2 should be allowed (different IP)")
	}
}

func TestIPRateLimiter_WindowExpiry(t *testing.T) {
	rl := NewIPRateLimiter(2, 50*time.Millisecond)
	defer rl.Stop()

	rl.Allow("1.2.3.4")
	rl.Allow("1.2.3.4")

	if rl.Allow("1.2.3.4") {
		t.Error("should be blocked before window expires")
	}

	time.Sleep(60 * time.Millisecond)

	if !rl.Allow("1.2.3.4") {
		t.Error("should be allowed after window expires")
	}
}

func TestIPRateLimiter_RetryAfter(t *testing.T) {
	rl := NewIPRateLimiter(1, time.Hour)
	defer rl.Stop()

	rl.Allow("1.2.3.4")

	retryAfter := rl.RetryAfter("1.2.3.4")
	if retryAfter < 3500 || retryAfter > 3601 {
		t.Errorf("RetryAfter should be ~3600, got %d", retryAfter)
	}
}

func TestIPRateLimitMiddleware_Returns429(t *testing.T) {
	gin.SetMode(gin.TestMode)

	rl := NewIPRateLimiter(1, time.Hour)
	defer rl.Stop()

	router := gin.New()
	router.Use(IPRateLimitMiddleware(rl))
	router.POST("/test", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"ok": true})
	})

	// First request — allowed
	req1 := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/test", http.NoBody)
	req1.Header.Set("X-Forwarded-For", "10.0.0.1")
	w1 := httptest.NewRecorder()
	router.ServeHTTP(w1, req1)
	if w1.Code != http.StatusOK {
		t.Errorf("first request: want 200, got %d", w1.Code)
	}

	// Second request — blocked
	req2 := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/test", http.NoBody)
	req2.Header.Set("X-Forwarded-For", "10.0.0.1")
	w2 := httptest.NewRecorder()
	router.ServeHTTP(w2, req2)
	if w2.Code != http.StatusTooManyRequests {
		t.Errorf("second request: want 429, got %d", w2.Code)
	}
	if w2.Header().Get("Retry-After") == "" {
		t.Error("expected Retry-After header")
	}
}

func TestIPRateLimitMiddleware_ForgedForwardedForDoesNotResetTheLimit(t *testing.T) {
	gin.SetMode(gin.TestMode)

	rl := NewIPRateLimiter(1, time.Hour)
	defer rl.Stop()

	router := gin.New()
	router.Use(IPRateLimitMiddleware(rl))
	router.POST("/test", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"ok": true})
	})

	codes := make([]int, 0, 2)
	for _, forged := range []string{"198.51.100.1", "198.51.100.2"} {
		req := httptest.NewRequestWithContext(context.Background(), http.MethodPost, "/test", http.NoBody)
		req.Header.Set("Fly-Client-IP", "203.0.113.9")
		req.Header.Set("X-Forwarded-For", forged+", 203.0.113.9")
		w := httptest.NewRecorder()
		router.ServeHTTP(w, req)
		codes = append(codes, w.Code)
	}
	if codes[0] != http.StatusOK || codes[1] != http.StatusTooManyRequests {
		t.Fatalf("want [200 429] keyed on Fly-Client-IP, got %v", codes)
	}
}
