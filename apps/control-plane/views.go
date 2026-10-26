package main

import (
	"io"
	"net/http"
	"strconv"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/domain/entities"
)

const (
	viewsPath             = "/api/v1/views"
	viewRoute             = viewsPath + "/:view"
	viewEntitiesRoute     = viewRoute + "/entities"
	viewEntityRoute       = viewEntitiesRoute + "/:key"
	viewChangesRoute      = viewRoute + "/changes"
	maxViewDefinition     = 64 * 1024
	maxViewNameLength     = 48
	maxViewKeyLength      = 512
	maxViewChangesWaitSec = 20
)

// registerViewRoutes mounts the tenant view routes on the /api/v1 group. The
// admin view surface is Core-only and is never mounted here.
func (cp *ControlPlane) registerViewRoutes(api *gin.RouterGroup) {
	read, write := RequirePermission(entities.PermissionRead), RequirePermission(entities.PermissionWrite)
	api.GET("/views", read, cp.ProxyView)
	api.GET("/views/:view", read, cp.ProxyView)
	api.PUT("/views/:view", write, cp.ProxyView)
	api.DELETE("/views/:view", write, cp.ProxyView)
	api.GET("/views/:view/entities", read, cp.ProxyView)
	api.GET("/views/:view/entities/:key", read, cp.ProxyView)
	api.GET("/views/:view/changes", read, cp.ProxyView)
}

func validViewName(name string) bool {
	if name == "" || len(name) > maxViewNameLength {
		return false
	}
	for _, c := range name {
		if (c < 'a' || c > 'z') && (c < '0' || c > '9') && c != '_' && c != '-' {
			return false
		}
	}
	return true
}

// viewUpstreamPath maps a matched route template to Core's path. The key is
// returned unescaped because forwardRequest escapes the whole path once.
func viewUpstreamPath(route, view, key string) (path, refusal string) {
	if route == viewsPath {
		return viewsPath, ""
	}
	if !validViewName(view) {
		return "", "invalid_view"
	}
	base := viewsPath + "/" + view
	switch route {
	case viewRoute:
		return base, ""
	case viewEntitiesRoute:
		return base + "/entities", ""
	case viewChangesRoute:
		return base + "/changes", ""
	case viewEntityRoute:
		if key == "" || key == "." || key == ".." || len(key) > maxViewKeyLength {
			return "", "invalid_view_key"
		}
		return base + "/entities/" + key, ""
	}
	return "", "invalid_view"
}

// viewChangesWait parses the long-poll wait in seconds and clamps it to the
// contract's 0..20. An unparseable value is left for Core to refuse.
func viewChangesWait(raw string) (int, bool) {
	seconds, err := strconv.Atoi(raw)
	if err != nil {
		return 0, false
	}
	return min(max(seconds, 0), maxViewChangesWaitSec), true
}

// ProxyView forwards the tenant view routes to Core. Core folds nothing here:
// it stores definitions and rows the Query Service writes (ADR-021).
func (cp *ControlPlane) ProxyView(c *gin.Context) {
	_, tenantID, _, ok := authIdentityFromContext(c)
	if !ok {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "unauthorized"})
		return
	}
	route := c.FullPath()
	path, invalid := viewUpstreamPath(route, c.Param("view"), c.Param("key"))
	if invalid != "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": invalid, "detail": "view names are 1..48 characters of [a-z0-9_-]; keys are 1..512 bytes"})
		return
	}
	var body []byte
	if c.Request.Method == http.MethodPut {
		var err error
		body, err = io.ReadAll(io.LimitReader(c.Request.Body, maxViewDefinition+1))
		if err != nil || len(body) > maxViewDefinition {
			c.JSON(http.StatusRequestEntityTooLarge, gin.H{"error": "view_definition_too_large"})
			return
		}
	}
	bearer, ok := cp.mintBearer(c)
	if !ok {
		return
	}
	query := c.Request.URL.Query()
	query.Del("tenant_id")
	if route == viewChangesRoute && query.Has("wait") {
		if seconds, ok := viewChangesWait(query.Get("wait")); ok {
			query.Set("wait", strconv.Itoa(seconds))
		}
	}
	var meter func() error
	if c.Request.Method == http.MethodGet {
		meter = func() error { return cp.recordQueryUsage(c.Request.Context(), tenantID) }
	}
	cp.delegation.forwardRequestChecked(c, c.Request.Method, cp.delegation.coreForTenant(tenantID), path, query, body, bearer, nil, meter)
}
