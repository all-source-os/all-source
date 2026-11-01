// Package corehold lets a gateway call wait through a short Core restart
// without ever resending a request that a Core handler may have run.
package corehold

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"sync"
	"time"
)

// Deadline bounds how long one call waits for Core to come back. It must stay
// below the 10s egress request deadline in enforceEgress: a delegated read
// waits inside that deadline while holding one of the 16 egress response slots.
const Deadline = 6 * time.Second

// RetryAfter is the Retry-After a gateway sends when Core stays unreachable
// for the whole Deadline.
const RetryAfter = "2"

const (
	firstBackoff = 50 * time.Millisecond
	maxBackoff   = 500 * time.Millisecond
	peekLimit    = 4096
)

// ErrUnreachable marks a call that gave up because Core never accepted it.
var ErrUnreachable = errors.New("core unreachable")

// Unreached reports whether err proves the request never reached a Core
// handler: the connection was never established.
func Unreached(err error) bool {
	if err == nil {
		return false
	}
	var op *net.OpError
	if errors.As(err, &op) && op.Op == "dial" {
		return true
	}
	var dns *net.DNSError
	return errors.As(err, &dns)
}

// NotReady reports whether a Core reply is the booting server's refusal,
// which Core sends before any handler runs.
func NotReady(status int, body []byte) bool {
	if status != http.StatusServiceUnavailable {
		return false
	}
	var payload struct {
		Error string `json:"error"`
	}
	return json.Unmarshal(body, &payload) == nil && payload.Error == "not_ready"
}

// ResponseNotReady applies NotReady to a live response. It reads at most a
// small prefix of a 503 body and puts it back, so the response can still be
// forwarded whole.
func ResponseNotReady(resp *http.Response) bool {
	if resp == nil || resp.StatusCode != http.StatusServiceUnavailable {
		return false
	}
	prefix, err := io.ReadAll(io.LimitReader(resp.Body, peekLimit))
	resp.Body = peekedBody{Reader: io.MultiReader(bytes.NewReader(prefix), resp.Body), Closer: resp.Body}
	return err == nil && NotReady(resp.StatusCode, prefix)
}

type peekedBody struct {
	io.Reader
	io.Closer
}

// Retryable is the retry rule for a request: a request that never reached a
// handler may always be resent, and an idempotent one may also be resent
// after a 5xx or a transport error.
func Retryable(method string, status int, body []byte, err error) bool {
	if Unreached(err) || NotReady(status, body) {
		return true
	}
	if !idempotent(method) {
		return false
	}
	return err != nil || status >= http.StatusInternalServerError
}

func idempotent(method string) bool {
	switch method {
	case http.MethodGet, http.MethodHead, http.MethodOptions:
		return true
	}
	return false
}

// Hold runs attempt until it returns false, ctx ends or Deadline passes, and
// reports whether it stopped while attempt still asked to retry. attempt must
// return true only when Core cannot have run a handler for the request.
func Hold(ctx context.Context, attempt func() bool) bool {
	deadline := time.Now().Add(Deadline)
	backoff := firstBackoff
	for {
		if !attempt() {
			MarkUp()
			return false
		}
		markDown()
		remaining := time.Until(deadline)
		if remaining <= 0 {
			return true
		}
		timer := time.NewTimer(min(backoff, remaining))
		select {
		case <-ctx.Done():
			timer.Stop()
			return true
		case <-timer.C:
		}
		backoff = min(2*backoff, maxBackoff)
	}
}

// availability is shared by every call in the process, so one call that finds
// Core down makes later requests wait in WaitUp, before they take a response
// slot, instead of each holding a slot through the restart.
var availability struct {
	mu       sync.Mutex
	up       chan struct{} // nil while Core is reachable; closed when it comes back
	lastDown time.Time
}

// probeAfter lets a waiter through when no call has tried Core for that long;
// otherwise, once every holder gave up, nothing would try Core again and
// waiters would never see it come back.
const probeAfter = time.Second

func markDown() {
	availability.mu.Lock()
	defer availability.mu.Unlock()
	if availability.up == nil {
		availability.up = make(chan struct{})
	}
	availability.lastDown = time.Now()
}

// MarkUp records that a call reached Core and releases every WaitUp.
func MarkUp() {
	availability.mu.Lock()
	defer availability.mu.Unlock()
	if availability.up != nil {
		close(availability.up)
		availability.up = nil
	}
}

// WaitUp returns at once while Core is reachable. While the last call found it
// down, WaitUp waits until a call gets through, ctx ends or Deadline passes, and
// reports whether the caller should go ahead and call Core.
func WaitUp(ctx context.Context) bool {
	deadline := time.Now().Add(Deadline)
	ticker := time.NewTicker(probeAfter / 4)
	defer ticker.Stop()
	for {
		availability.mu.Lock()
		up, idle := availability.up, time.Since(availability.lastDown) >= probeAfter
		availability.mu.Unlock()
		if up == nil || idle {
			return true
		}
		select {
		case <-up:
			return true
		case <-ctx.Done():
			return false
		case <-ticker.C:
		}
		if time.Now().After(deadline) {
			return false
		}
	}
}
