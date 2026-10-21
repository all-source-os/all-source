package main

import (
	"errors"
	"net/http"

	"github.com/gin-gonic/gin"

	"github.com/allsource/control-plane/internal/application/usecases"
)

const changelogSubscribedMessage = "You're on the list. We'll send the changelog and benchmark notes when they ship."

// ChangelogSubscribeHandler accepts a public changelog signup. It never logs or
// echoes the address, and answers new and repeat signups identically.
func (cp *ControlPlane) ChangelogSubscribeHandler(c *gin.Context) {
	var req usecases.SubscribeChangelogRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{
			"error": "invalid_subscription", "message": "Enter a valid email and accept the consent text.",
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

	if err := cp.container.ChangelogUC.Subscribe(c.Request.Context(), req); err != nil {
		changelogError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "subscribed", "message": changelogSubscribedMessage})
}

// ChangelogSubscribersHandler returns the private admin projection.
func (cp *ControlPlane) ChangelogSubscribersHandler(c *gin.Context) {
	subscribers, err := cp.container.ChangelogUC.List(c.Request.Context())
	if err != nil {
		changelogError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"subscribers": subscribers, "count": len(subscribers)})
}

type changelogUnsubscribeRequest struct {
	ID    string `json:"id"`
	Token string `json:"token"`
}

// ChangelogUnsubscribeWithTokenHandler is the public one-click unsubscribe
// behind the link in every changelog email.
func (cp *ControlPlane) ChangelogUnsubscribeWithTokenHandler(c *gin.Context) {
	var req changelogUnsubscribeRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_link", "message": "This unsubscribe link is not valid."})
		return
	}
	if err := cp.container.ChangelogUC.UnsubscribeWithToken(c.Request.Context(), req.ID, req.Token); err != nil {
		if errors.Is(err, usecases.ErrChangelogInvalidInput) {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid_link", "message": "This unsubscribe link is not valid."})
			return
		}
		changelogError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "unsubscribed", "message": "You're unsubscribed. We won't email you again."})
}

// ChangelogUnsubscribeHandler records an operator-handled unsubscribe request.
func (cp *ControlPlane) ChangelogUnsubscribeHandler(c *gin.Context) {
	if err := cp.container.ChangelogUC.Unsubscribe(c.Request.Context(), c.Param("id")); err != nil {
		changelogError(c, err)
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "unsubscribed"})
}

func changelogError(c *gin.Context, err error) {
	switch {
	case errors.Is(err, usecases.ErrChangelogInvalidInput):
		c.JSON(http.StatusBadRequest, gin.H{
			"error": "invalid_subscription", "message": "Enter a valid email and accept the consent text.",
		})
	case errors.Is(err, usecases.ErrChangelogNotFound):
		c.JSON(http.StatusNotFound, gin.H{"error": "not_found", "message": "Subscriber not found."})
	default:
		c.JSON(http.StatusServiceUnavailable, gin.H{
			"error": "subscription_unavailable", "message": "Signups are temporarily unavailable. Please try again later.",
		})
	}
}
