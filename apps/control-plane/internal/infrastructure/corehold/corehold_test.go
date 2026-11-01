package corehold

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/url"
	"strings"
	"testing"
	"time"
)

func TestUnreachedOnlyForFailuresBeforeConnect(t *testing.T) {
	dial := &net.OpError{Op: "dial", Net: "tcp", Err: errors.New("connection refused")}
	cases := []struct {
		name string
		err  error
		want bool
	}{
		{"dial refused wrapped by http client", &url.Error{Op: "Post", URL: "http://core", Err: dial}, true},
		{"dns failure", fmt.Errorf("call: %w", &net.DNSError{Err: "no such host", Name: "core"}), true},
		{"reset after connect", &net.OpError{Op: "read", Net: "tcp", Err: errors.New("connection reset by peer")}, false},
		{"eof", io.EOF, false},
		{"deadline after connect", context.DeadlineExceeded, false},
		{"nil", nil, false},
	}
	for _, tc := range cases {
		if got := Unreached(tc.err); got != tc.want {
			t.Errorf("%s: got %v want %v", tc.name, got, tc.want)
		}
	}
}

func TestNotReadyNeedsTheBootingRefusal(t *testing.T) {
	if !NotReady(http.StatusServiceUnavailable, []byte(`{"error":"not_ready"}`)) {
		t.Error("not_ready 503 not recognized")
	}
	if NotReady(http.StatusServiceUnavailable, []byte(`{"error":"overloaded"}`)) {
		t.Error("other 503 treated as not_ready")
	}
	if NotReady(http.StatusInternalServerError, []byte(`{"error":"not_ready"}`)) {
		t.Error("500 treated as not_ready")
	}
}

func TestResponseNotReadyKeepsTheBodyReadable(t *testing.T) {
	body := `{"error":"overloaded","detail":"` + strings.Repeat("x", 2*peekLimit) + `"}`
	resp := &http.Response{StatusCode: http.StatusServiceUnavailable, Body: io.NopCloser(strings.NewReader(body))}
	if ResponseNotReady(resp) {
		t.Fatal("overloaded treated as not_ready")
	}
	got, err := io.ReadAll(resp.Body)
	if err != nil || string(got) != body {
		t.Fatalf("body lost after peek: %d bytes, %v", len(got), err)
	}
}

func TestHoldStopsAtContextEnd(t *testing.T) {
	t.Cleanup(MarkUp)
	ctx, cancel := context.WithTimeout(t.Context(), 120*time.Millisecond)
	defer cancel()
	attempts := 0
	start := time.Now()
	if !Hold(ctx, func() bool { attempts++; return true }) {
		t.Fatal("hold reported success while every attempt asked to retry")
	}
	if elapsed := time.Since(start); elapsed > time.Second {
		t.Fatalf("hold outlived its context: %s", elapsed)
	}
	if attempts < 2 {
		t.Fatalf("attempts %d", attempts)
	}
}

func TestHoldReturnsOnFirstDefinitiveAnswer(t *testing.T) {
	attempts := 0
	if Hold(t.Context(), func() bool { attempts++; return attempts < 3 }) {
		t.Fatal("hold reported exhaustion after a definitive answer")
	}
	if attempts != 3 {
		t.Fatalf("attempts %d", attempts)
	}
}

func TestWaitUpReturnsAtOnceWhileCoreIsReachable(t *testing.T) {
	MarkUp()
	start := time.Now()
	if !WaitUp(t.Context()) {
		t.Fatal("WaitUp refused while Core was reachable")
	}
	if elapsed := time.Since(start); elapsed > 50*time.Millisecond {
		t.Fatalf("WaitUp waited %s with Core reachable", elapsed)
	}
}

func TestWaitUpReturnsWhenACallGetsThrough(t *testing.T) {
	t.Cleanup(MarkUp)
	markDown()
	go func() {
		time.Sleep(100 * time.Millisecond)
		MarkUp()
	}()
	start := time.Now()
	if !WaitUp(t.Context()) {
		t.Fatal("WaitUp refused after Core came back")
	}
	if elapsed := time.Since(start); elapsed > probeAfter/2 {
		t.Fatalf("WaitUp took %s after MarkUp", elapsed)
	}
}

func TestWaitUpLetsAProbeThroughWhenNoCallIsTrying(t *testing.T) {
	t.Cleanup(MarkUp)
	markDown()
	start := time.Now()
	if !WaitUp(t.Context()) {
		t.Fatal("WaitUp never let a probe through")
	}
	if elapsed := time.Since(start); elapsed < probeAfter/2 || elapsed > 2*probeAfter {
		t.Fatalf("probe released after %s, want about %s", elapsed, probeAfter)
	}
}

func TestWaitUpStopsAtContextEndWhileCoreStaysDown(t *testing.T) {
	t.Cleanup(MarkUp)
	markDown()
	stop := make(chan struct{})
	stopped := make(chan struct{})
	go func() {
		defer close(stopped)
		for {
			markDown()
			select {
			case <-stop:
				return
			case <-time.After(50 * time.Millisecond):
			}
		}
	}()
	t.Cleanup(func() { close(stop); <-stopped })
	ctx, cancel := context.WithTimeout(t.Context(), 200*time.Millisecond)
	defer cancel()
	if WaitUp(ctx) {
		t.Fatal("WaitUp let a caller through while holders kept finding Core down")
	}
}
