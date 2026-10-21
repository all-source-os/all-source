package main

import (
	"context"
	"fmt"
	"net/url"
	"strings"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
)

var queryMeterWrites = promauto.NewCounterVec(prometheus.CounterOpts{
	Name: "control_plane_query_meter_writes_total",
	Help: "Durable query meter writes, by success or failure.",
}, []string{"outcome"})

var queryResponseBytes = promauto.NewCounter(prometheus.CounterOpts{
	Name: "control_plane_event_query_response_bytes_total",
	Help: "Uncompressed event-query response body bytes written to clients; excludes HTTP headers and TLS.",
})

func (cp *ControlPlane) recordQueryUsage(_ context.Context, tenantID string) error {
	if cp.queryMeter == nil {
		return fmt.Errorf("query meter is not configured")
	}
	return cp.queryMeter.record(tenantID)
}

func (cp *ControlPlane) persistQueryUsage(ctx context.Context, tenantID string, count uint64) error {
	if cp.client == nil {
		return fmt.Errorf("query meter client is not configured")
	}
	target := *cp.delegation.coreForTenant(tenantID)
	target.Path = strings.TrimRight(target.Path, "/") + "/api/v1/tenants/" + url.PathEscape(tenantID) + "/usage/increment"
	response, err := cp.client.R().SetContext(ctx).
		SetBody(map[string]any{"type": "queries", "count": count}).Post(target.String())
	if err != nil {
		return fmt.Errorf("record query usage: %w", err)
	}
	if !response.IsSuccess() {
		return fmt.Errorf("record query usage: Core returned %d", response.StatusCode())
	}
	return nil
}
