package main

import (
	"net/http"
	"net/url"
	"strings"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
)

func (cp *ControlPlane) egressUsage(c *gin.Context) {
	tenant, ok := authTenantFromContext(c)
	if !ok {
		c.AbortWithStatus(http.StatusUnauthorized)
		return
	}
	target := *cp.delegation.coreForTenant(tenant)
	target.Path = strings.TrimRight(target.Path, "/") + "/api/v1/tenants/" + url.PathEscape(tenant) + "/egress"
	var value map[string]any
	response, err := cp.client.R().SetContext(c.Request.Context()).SetResult(&value).Get(target.String())
	if err != nil || !response.IsSuccess() {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "egress_usage_unavailable"})
		return
	}
	c.JSON(http.StatusOK, gin.H{"usage": value, "prepaid_packs": usecases.EgressPacks, "automatic_top_up": false, "unit": "serialized_response_body_bytes_before_compression", "allowance_period": "UTC_calendar_month", "prepaid_status": "requires_verified_product_configuration"})
}

func (cp *ControlPlane) egressCheckout(c *gin.Context) {
	tenant, ok := authTenantFromContext(c)
	if !ok {
		c.AbortWithStatus(http.StatusUnauthorized)
		return
	}
	var request struct {
		Pack string `json:"pack" binding:"required,oneof=100gb 1tb"`
	}
	if err := c.ShouldBindJSON(&request); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "select_prepaid_pack"})
		return
	}
	if cp.egressPrepaid == nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "prepaid_checkout_unavailable"})
		return
	}
	result, err := cp.egressPrepaid.Checkout(c.Request.Context(), tenant, request.Pack, "")
	if err != nil {
		c.JSON(http.StatusServiceUnavailable, gin.H{"error": "prepaid_checkout_unavailable", "message": "Prepaid checkout is not configured or its live product could not be verified. Contact your workspace owner."})
		return
	}
	c.JSON(http.StatusOK, result)
}
