package main

import (
	"encoding/json"
	"errors"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"testing"
	"time"

	"github.com/go-resty/resty/v2"

	"github.com/allsource/control-plane/internal/domain/entities"
)

const workspaceTestSecret = "synthetic-workspace-core-secret-only-2026"

type workspaceCore struct {
	binary, directory, url, port, role string
	cmd                                *exec.Cmd
	done                               chan error
}

func newWorkspaceCore(t *testing.T) *workspaceCore {
	t.Helper()
	binary := os.Getenv("ALLSOURCE_CORE_BINARY")
	if binary == "" {
		t.Skip("set ALLSOURCE_CORE_BINARY for actual Core proof")
	}
	config := net.ListenConfig{}
	listener, err := config.Listen(t.Context(), "tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	address, ok := listener.Addr().(*net.TCPAddr)
	if !ok {
		t.Fatal("loopback listener returned a non-TCP address")
	}
	port := strconv.Itoa(address.Port)
	if err := listener.Close(); err != nil {
		t.Fatal(err)
	}
	core := &workspaceCore{binary: binary, directory: t.TempDir(), port: port, role: "leader", url: "http://127.0.0.1:" + port}
	if err := os.Chmod(core.directory, 0o700); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { core.stop(t) })
	core.start(t)
	return core
}

func (core *workspaceCore) start(t *testing.T) {
	t.Helper()
	core.cmd = exec.CommandContext(t.Context(), core.binary) //nolint:gosec // opt-in test binary, no shell or external service
	core.cmd.Dir = core.directory
	core.cmd.Env = append(os.Environ(),
		"ALLSOURCE_HOST=127.0.0.1", "ALLSOURCE_PORT="+core.port,
		"ALLSOURCE_DATA_DIR="+core.directory,
		"ALLSOURCE_SYSTEM_DATA_DIR="+filepath.Join(core.directory, "system"),
		"ALLSOURCE_JWT_SECRET="+workspaceTestSecret,
		"ALLSOURCE_DEV_MODE=false", "ALLSOURCE_AUTH_DISABLED=false",
		"ALLSOURCE_ROLE="+core.role, "ALLSOURCE_REPLICATION_ENABLED=false", "ALLSOURCE_READ_ONLY=false",
		"ALLSOURCE_CLUSTER_ENABLED=false", "ALLSOURCE_BOOTSTRAP_API_KEY=",
		"ALLSOURCE_BOOTSTRAP_TENANT=", "ALLSOURCE_RESP_PORT=", "RUST_LOG=error",
	)
	core.cmd.Stdout = io.Discard
	core.cmd.Stderr = io.Discard
	if err := core.cmd.Start(); err != nil {
		t.Fatal("unable to start owned Core process")
	}
	core.done = make(chan error, 1)
	cmd, done := core.cmd, core.done
	go func() { done <- cmd.Wait() }()
	client := &http.Client{Timeout: 200 * time.Millisecond}
	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		select {
		case <-core.done:
			core.cmd = nil
			t.Fatal("owned Core process exited before readiness")
		default:
		}
		request, err := http.NewRequestWithContext(t.Context(), http.MethodGet, core.url+"/health", http.NoBody)
		if err != nil {
			t.Fatal("invalid readiness URL")
		}
		response, err := client.Do(request)
		if err == nil {
			if err := response.Body.Close(); err != nil {
				t.Fatal("readiness body did not close")
			}
			if response.StatusCode == 200 {
				return
			}
		}
		time.Sleep(25 * time.Millisecond)
	}
	t.Fatal("owned Core process did not become ready")
}

func (core *workspaceCore) stop(t *testing.T) {
	t.Helper()
	if core.cmd == nil {
		return
	}
	if err := core.cmd.Process.Kill(); err != nil && !errors.Is(err, os.ErrProcessDone) {
		t.Error("unable to kill owned Core process")
	}
	select {
	case <-core.done:
		core.cmd = nil
	case <-time.After(5 * time.Second):
		t.Fatal("owned Core process did not stop")
	}
}

func (core *workspaceCore) restart(t *testing.T) {
	t.Helper()
	core.stop(t)
	core.start(t)
}

func (core *workspaceCore) controlPlane(t *testing.T, origin string) *ControlPlane {
	t.Helper()
	auth := NewAuthClient(workspaceTestSecret, origin)
	token, err := auth.SignAPIKey("synthetic-admin", "test", entities.RoleAdmin)
	if err != nil {
		t.Fatal("unable to sign synthetic service credential")
	}
	client := resty.New().SetBaseURL(origin).SetAuthToken(token).SetTimeout(5 * time.Second).
		SetResponseBodyLimit(65_536).SetRedirectPolicy(resty.NoRedirectPolicy())
	return &ControlPlane{client: client, authClient: auth}
}

func workspaceJSON(t *testing.T, client *resty.Client, method, path string, body any, status int) map[string]any {
	t.Helper()
	request := client.R().SetContext(t.Context())
	if body != nil {
		request.SetBody(body)
	}
	response, err := request.Execute(method, path)
	if err != nil {
		t.Fatal("synthetic Core HTTP request failed")
	}
	if response.StatusCode() != status {
		t.Fatalf("Core status %d; expected %d", response.StatusCode(), status)
	}
	if status >= 400 || status == http.StatusNoContent {
		return nil
	}
	var result map[string]any
	if json.Unmarshal(response.Body(), &result) != nil {
		t.Fatal("Core response was not JSON")
	}
	return result
}

func setWorkspaceConfig(t *testing.T, cp *ControlPlane, key string, value any) {
	t.Helper()
	workspaceJSON(t, cp.client, http.MethodPost, "/api/v1/config", map[string]any{
		"key": key, "value": value, "changed_by": "synthetic-test",
	}, 200)
}

func assertWorkspaceMember(t *testing.T, cp *ControlPlane, tenant, subject, role string) {
	t.Helper()
	value, found, err := cp.workspaceConfig(t.Context(), teamMembersConfigKey(tenant))
	if err != nil || !found {
		t.Fatal("durable membership missing")
	}
	var members []TeamMember
	if json.Unmarshal(value, &members) != nil || len(members) != 1 || members[0].UserID != subject || members[0].Role != role {
		t.Fatal("unexpected durable membership")
	}
}

func workspaceMap(t *testing.T, value any) map[string]any {
	t.Helper()
	result, ok := value.(map[string]any)
	if !ok {
		t.Fatal("expected a JSON object")
	}
	return result
}

func workspaceBytes(t *testing.T, value any) []byte {
	t.Helper()
	result, err := json.Marshal(value)
	if err != nil {
		t.Fatal("unable to encode synthetic value")
	}
	return result
}
