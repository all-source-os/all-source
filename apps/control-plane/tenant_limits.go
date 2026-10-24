package main

import (
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
)

const tenantLimitsPath = "/api/v1/tenants/me/limits"

type bucketView struct {
	Tokens          float64 `json:"tokens"`
	Capacity        float64 `json:"capacity"`
	RefillPerSecond float64 `json:"refill_per_second"`
}

func viewBucket(b tokenBucket) bucketView {
	return bucketView{Tokens: b.tokens, Capacity: b.capacity, RefillPerSecond: b.rate}
}

// snapshot reads in-memory state only; it never creates tenant state or calls Core.
func (a *tenantAdmission) snapshot(tenant string, now time.Time) gin.H {
	mode, limits, override := admissionOff, defaultAdmissionLimits(), false
	var state *tenantAdmissionState
	inFlight, wouldRefuse := 0, map[string]uint64{}
	if a != nil {
		a.mu.Lock()
		defer a.mu.Unlock()
		mode = a.mode
		limits, override = a.limitsFor(tenant)
		state = a.tenants[tenant]
	}
	view := *newAdmissionState(limits, now)
	if state != nil {
		view = *state
		view.refill(now)
		inFlight = state.inFlight
		for scope, count := range state.wouldRefuse {
			wouldRefuse[scope] = count
		}
	}
	buckets := gin.H{"data": viewBucket(view.data), "meta": viewBucket(view.meta), "write": viewBucket(view.write), "bytes": viewBucket(view.bytes)}
	return gin.H{
		"mode":         mode,
		"override":     override,
		"limits":       limits,
		"buckets":      buckets,
		"in_flight":    inFlight,
		"would_refuse": wouldRefuse,
		"global": gin.H{
			"slots":      cap(egressResponseSlots),
			"occupied":   len(egressResponseSlots),
			"fair_share": gin.H{"occupancy": fairShareOccupancy, "max_per_tenant": fairShareSlots(cap(egressResponseSlots))},
		},
	}
}

// TenantLimits serves GET /api/v1/tenants/me/limits for the authenticated tenant.
func (cp *ControlPlane) TenantLimits(c *gin.Context) {
	tenant, ok := authTenantFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication_required"})
		return
	}
	c.Header("Cache-Control", "no-store")
	c.JSON(http.StatusOK, cp.admission.snapshot(tenant, time.Now()))
}
