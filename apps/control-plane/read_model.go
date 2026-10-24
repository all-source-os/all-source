package main

import (
	"io"
	"net/http"
	"net/url"

	"github.com/gin-gonic/gin"
)

const eventFeedPath = "/api/v1/event-feed"

func (cp *ControlPlane) ProxyReadModel(c *gin.Context) {
	_, tenantID, _, ok := authIdentityFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "unauthorized"})
		return
	}
	bearer, ok := cp.mintBearer(c)
	if !ok {
		return
	}
	path := eventFeedPath
	if name := c.Param("name"); name != "" {
		path = "/api/v1/read-models/" + url.PathEscape(name)
	}
	var body []byte
	if c.Request.Method == http.MethodPut {
		var err error
		body, err = io.ReadAll(io.LimitReader(c.Request.Body, 240*1024+1))
		if err != nil || len(body) > 240*1024 {
			c.JSON(http.StatusRequestEntityTooLarge, gin.H{"error": "read_model_commit_too_large"})
			return
		}
	}
	query := c.Request.URL.Query()
	query.Del("tenant_id")
	var meter func() error
	if c.Request.Method == http.MethodGet {
		meter = func() error { return cp.recordQueryUsage(c.Request.Context(), tenantID) }
	}
	cp.delegation.forwardRequestChecked(c, c.Request.Method, cp.delegation.coreForTenant(tenantID), path, query, body, bearer, nil, meter)
}
