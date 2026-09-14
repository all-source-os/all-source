package main

import (
	"errors"
	"net/http"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
	httphandlers "github.com/allsource/control-plane/internal/interfaces/http"
)

// DesignPartnerApplyHandler accepts a public design-partner application. It
// never logs or echoes applicant fields; only the opaque application ID leaves
// the private use-case boundary on success.
func (cp *ControlPlane) DesignPartnerApplyHandler(c *gin.Context) {
	var req usecases.SubmitDesignPartnerRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{
			"error": "invalid_application", "message": "Check the required fields and try again.",
		})
		return
	}

	if cp.turnstile != nil {
		if req.TurnstileResponse == "" {
			c.JSON(http.StatusForbidden, gin.H{
				"error": "captcha_required", "message": "Complete the spam check and try again.",
			})
			return
		}
		if err := cp.turnstile.Verify(c.Request.Context(), req.TurnstileResponse, clientIP(c)); err != nil {
			c.JSON(http.StatusForbidden, gin.H{
				"error": "captcha_failed", "message": "Spam verification failed. Please try again.",
			})
			return
		}
	}

	application, err := cp.container.DesignPartnerUC.Submit(c.Request.Context(), req)
	if err != nil {
		designPartnerError(c, err)
		return
	}
	c.JSON(http.StatusCreated, gin.H{
		"application_id": application.ID,
		"status":         application.Status,
		"message":        "Application received. We will reply by email within five business days.",
	})
}

// DesignPartnerApplicationsHandler returns the private admin projection.
func (cp *ControlPlane) DesignPartnerApplicationsHandler(c *gin.Context) {
	applications, err := cp.container.DesignPartnerUC.List(c.Request.Context(), c.Query("status"))
	if err != nil {
		designPartnerError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"applications": applications, "count": len(applications)})
}

// DesignPartnerStatusHandler appends one admin status change.
func (cp *ControlPlane) DesignPartnerStatusHandler(c *gin.Context) {
	var body struct {
		Status string `json:"status"`
		Note   string `json:"note,omitempty"`
	}
	if err := c.ShouldBindJSON(&body); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_status", "message": "Choose a valid status."})
		return
	}

	actor := "admin"
	if admin, err := httphandlers.GetAdminAuthContext(c); err == nil {
		switch {
		case admin.UserID != "":
			actor = admin.UserID
		case admin.Email != "":
			actor = admin.Email
		case admin.Username != "":
			actor = admin.Username
		}
	}
	application, err := cp.container.DesignPartnerUC.UpdateStatus(c.Request.Context(), usecases.UpdateDesignPartnerStatusRequest{
		ApplicationID: c.Param("id"), Status: body.Status, Actor: actor, Note: body.Note,
	})
	if err != nil {
		designPartnerError(c, err)
		return
	}
	c.JSON(http.StatusOK, application)
}

func designPartnerError(c *gin.Context, err error) {
	switch {
	case errors.Is(err, usecases.ErrDesignPartnerInvalidInput):
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_application", "message": err.Error()})
	case errors.Is(err, usecases.ErrDesignPartnerNotFound):
		c.JSON(http.StatusNotFound, gin.H{"error": "not_found", "message": "Application not found."})
	default:
		c.JSON(http.StatusServiceUnavailable, gin.H{
			"error": "application_unavailable", "message": "Applications are temporarily unavailable. Please try again later.",
		})
	}
}
