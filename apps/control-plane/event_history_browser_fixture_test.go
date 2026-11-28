package main

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/go-resty/resty/v2"

	"github.com/allsource/control-plane/internal/domain/entities"
)

const reviewCore = "http://127.0.0.1:14901"
const reviewLogin = "http://127.0.0.1:14903"
const reviewAuth = "http://127.0.0.1:14905"
const reviewEmail = "reviewer@example.test"

func reviewSecret(t *testing.T) string {
	t.Helper()
	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		t.Fatal(err)
	}
	return hex.EncodeToString(raw)
}

func reviewFile(t *testing.T, directory, name string, data any) {
	t.Helper()
	value, err := json.MarshalIndent(data, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(directory, name), value, 0o600); err != nil {
		t.Fatal(err)
	}
}

func reviewService(ctx context.Context, t *testing.T, binary string, args, env []string) {
	t.Helper()
	if !filepath.IsAbs(binary) {
		t.Fatal("review requires an absolute local test binary path")
	}
	limiter, err := exec.LookPath("gtimeout")
	if err != nil {
		limiter, err = exec.LookPath("timeout")
	}
	if err != nil {
		t.Fatal("install GNU timeout for bounded fixture processes")
	}
	command := exec.CommandContext(ctx, limiter, append([]string{"-k", "2", "280", binary}, args...)...) //nolint:gosec // Test binaries and paths are supplied by the local reviewer.
	command.Dir = t.TempDir()
	command.Env = append([]string{"PATH=" + os.Getenv("PATH"), "HOME=" + command.Dir}, env...)
	command.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
	command.Cancel = func() error {
		err := syscall.Kill(-command.Process.Pid, syscall.SIGTERM)
		if errors.Is(err, syscall.ESRCH) {
			return os.ErrProcessDone
		}
		return err
	}
	if err := command.Start(); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := command.Cancel(); err != nil && !errors.Is(err, os.ErrProcessDone) {
			t.Errorf("stop review process: %v", err)
		}
		if err := command.Wait(); err != nil && !errors.Is(err, ctx.Err()) {
			var exit *exec.ExitError
			if !errors.As(err, &exit) || (exit.ExitCode() != 143 && exit.Sys() != syscall.WaitStatus(syscall.SIGTERM)) {
				t.Errorf("review process exited unexpectedly: %v", err)
			}
		}
	})
}

func reviewReady(ctx context.Context, t *testing.T, origin string) {
	t.Helper()
	client := http.Client{Timeout: time.Second}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, origin+"/health", http.NoBody)
	if err != nil {
		t.Fatal(err)
	}
	deadline := time.Now().Add(15 * time.Second)
	for ctx.Err() == nil && time.Now().Before(deadline) {
		res, err := client.Do(request)
		if err == nil {
			if err := res.Body.Close(); err != nil {
				t.Fatal(err)
			}
			if res.StatusCode == http.StatusOK {
				return
			}
		}
		time.Sleep(80 * time.Millisecond)
	}
	t.Fatal("fixture did not become ready at " + origin)
}

func TestEventHistoryReviewFixture(t *testing.T) {
	if os.Getenv("ALLSOURCE_HISTORY_REVIEW_FIXTURE") != "1" {
		t.Skip("isolated reviewer only")
	}
	origin := os.Getenv("ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN")
	parsed, err := url.Parse(origin)
	if err != nil || parsed.User != nil || parsed.Host == "" || parsed.String() != parsed.Scheme+"://"+parsed.Host ||
		(parsed.Scheme != "https" && origin != "http://127.0.0.1:14900") {
		t.Fatal("supply the isolated public HTTPS origin or local review origin")
	}
	ctx, stop := signal.NotifyContext(t.Context(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	var listeners net.ListenConfig
	for _, port := range []string{"14900", "14901", "14902", "14903", "14904", "14905", "14909"} {
		l, err := listeners.Listen(ctx, "tcp4", "127.0.0.1:"+port)
		if err != nil {
			t.Fatal("review port occupied: " + port)
		}
		if err := l.Close(); err != nil {
			t.Fatal(err)
		}
	}
	directory := t.TempDir()
	jwtKey, password := reviewSecret(t), reviewSecret(t)
	const gateway = "history-review-gateway"
	reviewService(ctx, t, os.Getenv("ALLSOURCE_CORE_BINARY"), nil, []string{
		"ALLSOURCE_HOST=127.0.0.1", "ALLSOURCE_PORT=14901", "ALLSOURCE_ROLE=leader", "ALLSOURCE_JWT_SECRET=" + jwtKey,
		"ALLSOURCE_SYSTEM_DATA_DIR=" + filepath.Join(directory, "system"), "ALLSOURCE_HISTORY_GATEWAY_SUBJECT=" + gateway,
		"ALLSOURCE_AUTH_DISABLED=false", "ALLSOURCE_DEV_MODE=false", "ALLSOURCE_RESP_PORT=", "ALLSOURCE_ARCHIVE_WORKERS=1",
		"ALLSOURCE_REPLICATION_ENABLED=false", "ALLSOURCE_CLUSTER_ENABLED=false", "RUST_LOG=error",
	})
	reviewReady(ctx, t, reviewCore)
	reviewService(ctx, t, os.Getenv("ALLSOURCE_AUTH_FIXTURE_BINARY"), []string{"--ignored", "--exact", "auth_module::event_history_review::serve"},
		[]string{"HISTORY_FIXTURE_PASSWORD=" + password})
	reviewReady(ctx, t, reviewAuth)
	t.Setenv("AUTH_SERVICE_URL", reviewAuth)
	auth := NewAuthClient(jwtKey, reviewCore)
	service, err := auth.SignAPIKey(gateway, "Review gateway", entities.RoleAdmin)
	if err != nil {
		t.Fatal("could not initialize synthetic gateway identity")
	}
	client := resty.New().SetBaseURL(reviewCore).SetAuthToken(service).SetTimeout(5 * time.Second).
		SetResponseBodyLimit(65536).SetRedirectPolicy(resty.NoRedirectPolicy())
	cp := &ControlPlane{client: client, authClient: auth}
	router := gin.New()
	router.POST("/api/v1/auth/login", cp.LoginHandler)
	login := &http.Server{Handler: router, ReadHeaderTimeout: 3 * time.Second, ReadTimeout: 5 * time.Second, WriteTimeout: 8 * time.Second}
	listener, err := listeners.Listen(ctx, "tcp4", "127.0.0.1:14903")
	if err != nil {
		t.Fatal(err)
	}
	served := make(chan error, 1)
	go func() { served <- login.Serve(listener) }()
	t.Cleanup(func() {
		if err := login.Close(); err != nil {
			t.Errorf("close review login: %v", err)
		}
		if err := <-served; !errors.Is(err, http.ErrServerClosed) {
			t.Errorf("login shutdown: %v", err)
		}
	})
	local := resty.New().SetBaseURL(reviewLogin).SetTimeout(10 * time.Second).SetRedirectPolicy(resty.NoRedirectPolicy())
	identity := workspaceJSON(t, local, "POST", "/api/v1/auth/login", map[string]string{"email": reviewEmail, "password": password}, 200)
	token, ok := identity["token"].(string)
	if !ok {
		t.Fatal("product login did not establish a session")
	}
	claims, err := auth.ValidateToken(token)
	if err != nil || !claims.EmailVerified || claims.IsDemo || claims.Email != reviewEmail {
		t.Fatal("fixture account failed verified sign-in")
	}
	events := seedReviewWorkspaces(t, client, claims.UserID)
	reviewFile(t, directory, "events.json", events)
	reviewFile(t, directory, "login.json", map[string]string{"email": reviewEmail, "password": password})
	reviewFile(t, directory, "services.json", map[string]string{
		"ALLSOURCE_EVENT_HISTORY_ENABLED": "true", "ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN": origin, "NEXT_PUBLIC_APP_URL": origin,
		"ALLSOURCE_HISTORY_GATEWAY_SUBJECT": gateway, "JWT_SECRET": jwtKey, "SECRET_KEY_BASE": reviewSecret(t), "CORE_URL": reviewCore,
		"CORE_API_KEY": "Bearer " + service, "AUTH_DISABLED": "false", "QUERY_SERVICE_URL": "http://127.0.0.1:14902",
		"ALLSOURCE_EVENT_HISTORY_QUERY_URL": "http://127.0.0.1:14902", "ALLSOURCE_EVENT_HISTORY_HTTP_URL": "http://127.0.0.1:14904",
		"CONTROL_PLANE_INTERNAL_URL": reviewLogin, "CUSTOMER_REVIEW_HTTP_PORT": "14904", "CUSTOMER_REVIEW_HTTP_IP": "127.0.0.1",
	})
	webDir := os.Getenv("HISTORY_WEB_APP_DIR")
	if !filepath.IsAbs(webDir) {
		t.Fatal("HISTORY_WEB_APP_DIR must name the built review apps/web")
	}
	if _, err := os.Stat(filepath.Join(webDir, ".next", "BUILD_ID")); err != nil {
		t.Fatal("review web production build missing")
	}
	for _, filename := range []string{".env", ".env.local", ".env.production", ".env.production.local"} {
		if _, err := os.Stat(filepath.Join(webDir, filename)); !errors.Is(err, os.ErrNotExist) {
			t.Fatal("review web directory must not load local credentials")
		}
	}
	repo := filepath.Dir(filepath.Dir(webDir))
	node, err := exec.LookPath("node")
	if err != nil {
		t.Fatal("Node.js is required")
	}
	challenge := reviewSecret(t)
	reviewService(ctx, t, node, []string{filepath.Join(repo, "tooling", "e2e", "event-history-review", "web.cjs")}, []string{
		"NODE_ENV=production", "NEXT_TELEMETRY_DISABLED=1", "HISTORY_WEB_APP_DIR=" + webDir, "HISTORY_WEB_CHALLENGE=" + challenge,
		"ALLSOURCE_EVENT_HISTORY_ENABLED=true", "ALLSOURCE_EVENT_HISTORY_WEB_ORIGIN=" + origin, "NEXT_PUBLIC_APP_URL=" + origin,
		"QUERY_SERVICE_URL=http://127.0.0.1:14902", "NEXT_PUBLIC_API_URL=http://127.0.0.1:14902",
		"ALLSOURCE_EVENT_HISTORY_HTTP_URL=http://127.0.0.1:14904", "CONTROL_PLANE_INTERNAL_URL=" + reviewLogin,
		"AUTH_SERVICE_URL=" + reviewAuth, "NEXT_PUBLIC_POSTHOG_KEY=", "NEXT_PUBLIC_GA_MEASUREMENT_ID=",
		"ALLSOURCE_EVENT_HISTORY_ORIGINS=" + os.Getenv("ALLSOURCE_EVENT_HISTORY_ORIGINS"),
	})
	openReviewIngress(ctx, t, parsed, challenge)
	t.Logf("Synthetic workspace and restricted ingress ready. Private fixture directory: %s", directory)
	select {
	case <-ctx.Done():
	case <-time.After(240 * time.Second):
	}
}

func seedReviewWorkspaces(t *testing.T, client *resty.Client, subject string) []map[string]any {
	t.Helper()
	const home = "review-engineering"
	for _, tenant := range []string{home, "review-support", "review-other"} {
		workspaceJSON(t, client, "POST", "/api/v1/tenants", map[string]any{"id": tenant, "name": strings.ReplaceAll(tenant, "-", " ")}, 201)
		workspaceJSON(t, client, "PUT", "/api/v1/tenants/"+tenant, map[string]any{"metadata": map[string]any{
			"subscription": map[string]any{"tier": "indie", "status": "active"}, "quotas": map[string]any{"mcp_scope": "read", "queries_quota": 100, "queries_used": 0},
		}}, 200)
		member := subject
		if tenant == "review-other" {
			member = "foreign-reviewer"
		}
		workspaceJSON(t, client, "POST", "/api/v1/config", map[string]any{"key": "team:" + tenant + ":members", "value": map[string]any{
			"schema_version": 2, "members": []any{map[string]any{"user_id": member, "role": "member", "name": "Review engineer", "joined_at": time.Now().UTC().Format(time.RFC3339)}}, "invitations": map[string]any{},
		}, "changed_by": "review-fixture"}, 200)
	}
	var evidence []map[string]any
	seed := func(tenant, entity, kind string, change map[string]string) {
		reply := workspaceJSON(t, client, "POST", "/api/v1/events", map[string]any{"tenant_id": tenant, "event_type": kind, "entity_id": entity, "payload": change}, 200)
		if reply["event_id"] == nil || reply["recorded_at"] == nil {
			t.Fatal("missing immutable event ID or recorded time")
		}
		evidence = append(evidence, map[string]any{"workspace": tenant, "entity": entity, "event_type": kind, "event_id": reply["event_id"], "recorded_at": reply["recorded_at"], "change": change})
	}
	seed(home, "checkout", "deployment.released.v1", map[string]string{"version": "2.4"})
	seed(home, "billing", "deployment.released.v1", map[string]string{"version": "1.0"})
	seed("review-other", "checkout", "deployment.released.v1", map[string]string{"version": "foreign"})
	seed(home, "checkout", "deployment.rolled_back.v1", map[string]string{"from_version": "2.4", "to_version": "2.3"})
	for range 21 {
		seed(home, "catalog", "review.recorded", map[string]string{})
	}
	return evidence
}
