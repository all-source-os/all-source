package main

import (
	"context"
	"log"
	"time"

	"github.com/prometheus/client_golang/prometheus"

	"github.com/allsource/control-plane/internal/application/usecases/billing"
	"github.com/allsource/control-plane/internal/domain/entities"
)

const tenantPlanRefresh = 5 * time.Minute

// tenantPlanMonthlyUSD puts each tenant's list price beside its metered cost, so
// the business dashboard can show which tenants cost more than they pay.
var tenantPlanMonthlyUSD = tenantMetrics.NewGaugeVec(prometheus.GaugeOpts{
	Name: "control_plane_tenant_plan_monthly_usd",
	Help: "List price of the tenant's plan in US dollars per month; enterprise is negotiated and reads 0.",
}, []string{"tenant", "tier", "status"})

func recordTenantPlans(tenants []*entities.Tenant) {
	tenantPlanMonthlyUSD.Reset()
	for _, t := range tenants {
		tier, status, usd := billing.TenantPlan(t)
		if status == "" {
			status = "active"
		}
		tenantPlanMonthlyUSD.WithLabelValues(tenantLabel(t.ID), tier, status).Set(usd)
	}
}

func startTenantPlanMetrics(ctx context.Context, list func() ([]*entities.Tenant, error)) {
	refresh := func() {
		tenants, err := list()
		if err != nil {
			log.Printf("tenant plan metrics: list tenants: %v", err)
			return
		}
		recordTenantPlans(tenants)
	}
	go func() {
		refresh()
		ticker := time.NewTicker(tenantPlanRefresh)
		defer ticker.Stop()
		for {
			select {
			case <-ctx.Done():
				return
			case <-ticker.C:
				refresh()
			}
		}
	}()
}
