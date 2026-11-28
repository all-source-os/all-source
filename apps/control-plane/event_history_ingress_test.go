package main

import (
	"context"
	"crypto/subtle"
	"errors"
	"io"
	"log"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"path"
	"regexp"
	"strings"
	"testing"
	"time"
)

var reviewCitationPath = regexp.MustCompile(`^/event-history/workspaces/[A-Za-z0-9_-]{1,128}/events/[A-Fa-f0-9-]{36}$`)

func publicReviewRoute(method, target string) bool {
	switch target {
	case "/.well-known/oauth-protected-resource/mcp/event-history", "/.well-known/oauth-authorization-server/event-history",
		"/api/event-history/oauth/authorize", "/login", "/api/auth/session":
		return method == http.MethodGet
	case "/api/event-history/oauth/token", "/api/v1/auth/login":
		return method == http.MethodPost
	case "/mcp/event-history":
		return method == http.MethodGet || method == http.MethodPost || method == http.MethodDelete
	case "/event-history/connect", "/event-history/connections":
		return method == http.MethodGet || method == http.MethodPost
	}
	if reviewCitationPath.MatchString(target) {
		return method == http.MethodGet || method == http.MethodPost
	}
	if method != http.MethodGet || !strings.HasPrefix(target, "/_next/static/") || target != path.Clean(target) {
		return false
	}
	switch path.Ext(target) {
	case ".css", ".js", ".woff", ".woff2":
		return true
	}
	return false
}

func openReviewIngress(ctx context.Context, t *testing.T, origin *url.URL, challenge string) {
	t.Helper()
	upstream := &url.URL{Scheme: "http", Host: "127.0.0.1:14900"}
	transport := &http.Transport{Proxy: nil, DialContext: (&net.Dialer{Timeout: time.Second}).DialContext,
		ResponseHeaderTimeout: 25 * time.Second, MaxIdleConns: 12, IdleConnTimeout: 15 * time.Second}
	t.Cleanup(transport.CloseIdleConnections)
	private := &http.Client{Transport: transport, Timeout: time.Second, CheckRedirect: func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }}
	verified := func(ctx context.Context) bool {
		request, err := http.NewRequestWithContext(ctx, http.MethodGet, upstream.String()+"/.fixture/ready", http.NoBody)
		if err != nil {
			return false
		}
		r, err := private.Do(request)
		if err != nil {
			return false
		}
		raw, err := io.ReadAll(io.LimitReader(r.Body, 65))
		closeErr := r.Body.Close()
		return r.StatusCode == http.StatusOK && err == nil && closeErr == nil && subtle.ConstantTimeCompare([]byte(challenge), raw) == 1
	}
	deadline := time.Now().Add(25 * time.Second)
	for !verified(ctx) {
		if ctx.Err() != nil || time.Now().After(deadline) {
			t.Fatal("expected review web process did not bind to port 14900")
		}
		time.Sleep(100 * time.Millisecond)
	}
	proxy := &httputil.ReverseProxy{
		Transport: transport, ErrorLog: log.New(io.Discard, "", 0),
		ErrorHandler: func(w http.ResponseWriter, _ *http.Request, _ error) {
			http.Error(w, "Review unavailable", http.StatusServiceUnavailable)
		},
		Rewrite: func(r *httputil.ProxyRequest) {
			r.SetURL(upstream)
			r.Out.Host = origin.Host
			headers := make(http.Header)
			for _, key := range []string{"Accept", "Accept-Language", "Authorization", "Content-Type", "Cookie", "Origin", "Sec-Fetch-Mode", "Sec-Fetch-Site", "Sec-Fetch-Dest", "Mcp-Session-Id", "MCP-Protocol-Version", "Last-Event-ID"} {
				for _, value := range r.In.Header.Values(key) {
					headers.Add(key, value)
				}
			}
			headers.Set("X-Forwarded-Host", origin.Host)
			headers.Set("X-Forwarded-Proto", origin.Scheme)
			r.Out.Header = headers
		},
	}
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		p := r.URL.Path
		if p != path.Clean(p) || strings.ContainsAny(p, "\\\x00") || !publicReviewRoute(r.Method, p) {
			http.NotFound(w, r)
			return
		}
		if !verified(r.Context()) {
			http.Error(w, "Review unavailable", http.StatusServiceUnavailable)
			return
		}
		r.Body = http.MaxBytesReader(w, r.Body, 65536)
		proxy.ServeHTTP(w, r)
	})
	listener, err := new(net.ListenConfig).Listen(ctx, "tcp4", "127.0.0.1:14909")
	if err != nil {
		t.Fatal(err)
	}
	server := &http.Server{Handler: handler, ReadHeaderTimeout: 3 * time.Second, ReadTimeout: 15 * time.Second, WriteTimeout: 35 * time.Second, MaxHeaderBytes: 32768}
	exited := make(chan error, 1)
	go func() { exited <- server.Serve(listener) }()
	t.Cleanup(func() {
		if err := server.Close(); err != nil {
			t.Errorf("close review ingress: %v", err)
		}
		if err := <-exited; !errors.Is(err, http.ErrServerClosed) {
			t.Errorf("review ingress shutdown: %v", err)
		}
	})
}

func TestReviewIngressRoutes(t *testing.T) {
	event := "/event-history/workspaces/shop/events/00000000-0000-4000-b000-000000000001"
	for _, test := range []struct {
		method, path string
		want         bool
	}{
		{"GET", event, true}, {"POST", event, true}, {"DELETE", event, false}, {"GET", "/_next/static/chunks/login.js", true},
		{"GET", "/.fixture/ready", false}, {"POST", "/api/v1/auth/login", true}, {"GET", "/api/v1/auth/oauth/google", false},
		{"GET", "/_next/static/chunks/runtime.js.map", false}, {"GET", "/_next/static/../private.js", false},
		{"GET", "/dashboard", false}, {"POST", "/mcp/customer-review", false}, {"POST", "/api/event-history/evidence", false},
	} {
		if publicReviewRoute(test.method, test.path) != test.want {
			t.Errorf("%s %s: wrong ingress access", test.method, test.path)
		}
	}
}
