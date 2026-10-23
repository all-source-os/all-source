//go:build corerelease

package main

import (
	"fmt"
	"os"
	"slices"
	"strings"
	"testing"
	"time"
)

func TestEgressRealCoreReleaseReadPerformance(t *testing.T) {
	baseline := os.Getenv("ALLSOURCE_CORE_BASELINE_BINARY")
	candidate := os.Getenv("ALLSOURCE_CORE_BINARY")
	if baseline == "" || candidate == "" {
		t.Fatal("both baseline and candidate release binaries are required")
	}
	measure := func(binary string) float64 {
		t.Helper()
		t.Setenv("ALLSOURCE_CORE_BINARY", binary)
		core := newWorkspaceCore(t)
		defer core.stop(t)
		cp := core.controlPlane(t, core.url)
		workspaceJSON(t, cp.client, "POST", "/api/v1/tenants", map[string]any{
			"id": "release-benchmark", "name": "release-benchmark",
			"metadata": map[string]any{"subscription": map[string]any{"tier": "indie", "status": "active"}},
		}, 201)
		for i := range 32 {
			workspaceJSON(t, cp.client, "POST", "/api/v1/events", map[string]any{
				"tenant_id": "release-benchmark", "entity_id": fmt.Sprintf("workflow-%d", i),
				"event_type": "workflow.drafted", "payload": map[string]any{"state": "review", "text": strings.Repeat("x", 64)},
			}, 200)
		}
		read := func() {
			result := workspaceJSON(t, cp.client, "GET", "/api/v1/events/query?tenant_id=release-benchmark&limit=32&order=desc", nil, 200)
			if result["count"] != float64(32) {
				t.Fatalf("release read lost workflow events: count=%v", result["count"])
			}
		}
		for range 16 {
			read()
		}
		rates := make([]float64, 3)
		for i := range rates {
			start := time.Now()
			for range 256 {
				read()
			}
			rates[i] = 256 / time.Since(start).Seconds()
		}
		slices.Sort(rates)
		return rates[1]
	}
	baselineRate := measure(baseline)
	candidateRate := measure(candidate)
	ratio := candidateRate / baselineRate
	t.Logf("release reads/s: baseline=%.2f candidate=%.2f ratio=%.3f", baselineRate, candidateRate, ratio)
	if ratio < 0.8 {
		t.Fatalf("candidate release read throughput below 80%% of baseline: %.3f", ratio)
	}
}
