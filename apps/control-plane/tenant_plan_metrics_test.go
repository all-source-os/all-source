package main

import (
	"testing"

	"github.com/allsource/control-plane/internal/domain/entities"
)

func tenantPlanSeries(t *testing.T) map[string]float64 {
	t.Helper()
	families, err := tenantMetricsRegistry.Gather()
	if err != nil {
		t.Fatal(err)
	}
	series := map[string]float64{}
	for _, family := range families {
		if family.GetName() != "control_plane_tenant_plan_monthly_usd" {
			continue
		}
		for _, metric := range family.GetMetric() {
			labels := map[string]string{}
			for _, pair := range metric.GetLabel() {
				labels[pair.GetName()] = pair.GetValue()
			}
			series[labels["tenant"]+"|"+labels["tier"]+"|"+labels["status"]] = metric.GetGauge().GetValue()
		}
	}
	return series
}

func TestTenantPlanGaugeCarriesListPriceAndDropsDeletedTenants(t *testing.T) {
	plan := func(id, tier, status string) *entities.Tenant {
		return &entities.Tenant{ID: id, Metadata: map[string]interface{}{
			"subscription": map[string]interface{}{"tier": tier, "status": status},
		}}
	}
	recordTenantPlans([]*entities.Tenant{
		plan("plan-indie", "indie", "active"),
		plan("plan-scale", "scale", "past_due"),
		plan("plan-enterprise", "enterprise", "active"),
		{ID: "plan-unset"},
	})
	got := tenantPlanSeries(t)
	for key, want := range map[string]float64{
		"plan-indie|indie|active":           19,
		"plan-scale|scale|past_due":         299,
		"plan-enterprise|enterprise|active": 0,
		"plan-unset|free|active":            0,
	} {
		if value, ok := got[key]; !ok || value != want {
			t.Errorf("%s: %v (present %v), want %v", key, value, ok, want)
		}
	}

	recordTenantPlans([]*entities.Tenant{plan("plan-indie", "indie", "active")})
	if got := tenantPlanSeries(t); len(got) != 1 {
		t.Fatalf("series after tenants left: %v, want only plan-indie", got)
	}
}
