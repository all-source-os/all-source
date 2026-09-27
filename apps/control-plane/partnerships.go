package main

import (
	"errors"
	"net/http"

	"github.com/allsource/control-plane/internal/application/usecases"
	httphandlers "github.com/allsource/control-plane/internal/interfaces/http"
	"github.com/gin-gonic/gin"
)

func (cp *ControlPlane) PartnershipsListHandler(c *gin.Context) {
	c.Header("Cache-Control", "private, no-store")
	records, err := cp.container.PartnershipsUC.List(c.Request.Context())
	if err != nil {
		partnershipError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"records": records})
}

func (cp *ControlPlane) PartnershipsHistoryHandler(c *gin.Context) {
	c.Header("Cache-Control", "private, no-store")
	history, err := cp.container.PartnershipsUC.History(c.Request.Context(), c.Param("id"))
	if err != nil {
		partnershipError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"history": history})
}

func (cp *ControlPlane) PartnershipsSaveHandler(c *gin.Context) {
	c.Header("Cache-Control", "private, no-store")
	admin, err := httphandlers.GetAdminAuthContext(c)
	if err != nil {
		c.JSON(http.StatusUnauthorized, gin.H{"message": "Admin session required"})
		return
	}
	c.Request.Body = http.MaxBytesReader(c.Writer, c.Request.Body, 2<<20)
	var req usecases.SavePartnershipRequest
	if err = c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"message": "Invalid or oversized partnership record"})
		return
	}
	actor := admin.UserID
	if actor == "" {
		actor = admin.Username
	}
	if actor == "" {
		actor = admin.Email
	}
	saved, err := cp.container.PartnershipsUC.Save(c.Request.Context(), req, actor)
	if err != nil {
		partnershipError(c, err)
		return
	}
	c.JSON(http.StatusOK, saved)
}

func partnershipError(c *gin.Context, err error) {
	switch {
	case errors.Is(err, usecases.ErrPartnershipInvalid):
		c.JSON(http.StatusBadRequest, gin.H{"message": err.Error()})
	case errors.Is(err, usecases.ErrPartnershipConflict):
		c.JSON(http.StatusConflict, gin.H{"message": err.Error()})
	default:
		c.JSON(http.StatusServiceUnavailable, gin.H{"message": "Partnership records are unavailable. No successful save is confirmed."})
	}
}
