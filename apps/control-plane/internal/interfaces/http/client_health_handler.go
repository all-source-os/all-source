package http

import (
	"errors"
	"log"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
)

// ClientHealthHandler serves GET /api/v1/admin/metrics/client-health.
type ClientHealthHandler struct {
	uc *usecases.ClientHealthUseCase
}

// NewClientHealthHandler creates a ClientHealthHandler.
func NewClientHealthHandler(uc *usecases.ClientHealthUseCase) *ClientHealthHandler {
	return &ClientHealthHandler{uc: uc}
}

// Get returns the client-health report for ?range=1h|24h|7d (default 1h).
// Prometheus unconfigured ⇒ 200 with configured=false; Prometheus failing ⇒
// 502 with the error and no numbers.
func (h *ClientHealthHandler) Get(c *gin.Context) {
	rangeKey := c.DefaultQuery("range", "1h")
	report, err := h.uc.Report(c.Request.Context(), rangeKey)
	switch {
	case errors.Is(err, usecases.ErrInvalidHealthRange):
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
	case err != nil:
		log.Printf("client-health: %v", err)
		c.JSON(http.StatusBadGateway, usecases.ClientHealthReport{
			Configured:  true,
			Error:       err.Error(),
			Range:       rangeKey,
			GeneratedAt: time.Now().UTC().Format(time.RFC3339),
		})
	default:
		c.JSON(http.StatusOK, report)
	}
}
