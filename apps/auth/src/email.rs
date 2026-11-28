//! Resend Email Service for AllSource
//!
//! Direct integration with the Resend API for transactional emails.
//! Supports: welcome, password reset, email verification.
//! Config: `RESEND_API_KEY` and `RESEND_FROM_EMAIL` env vars.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Email service backed by the Resend API.
#[derive(Clone)]
pub struct EmailService {
    client: reqwest::Client,
    api_key: String,
    from_email: String,
    app_url: String,
}

#[derive(Debug, Serialize)]
struct SendEmailRequest<'a> {
    from: &'a str,
    to: Vec<&'a str>,
    subject: &'a str,
    html: &'a str,
}

#[derive(Debug, Deserialize)]
struct SendEmailResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ResendError {
    message: String,
}

impl EmailService {
    /// Create from environment variables.
    /// Returns None if `RESEND_API_KEY` is not set (graceful degradation).
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("RESEND_API_KEY").ok()?;
        if api_key.is_empty() {
            return None;
        }

        let from_email = std::env::var("RESEND_FROM_EMAIL")
            .unwrap_or_else(|_| "AllSource <noreply@allsource.io>".to_string());

        let app_url =
            std::env::var("APP_URL").unwrap_or_else(|_| "https://app.allsource.io".to_string());

        Some(Self {
            client: reqwest::Client::new(),
            api_key,
            from_email,
            app_url,
        })
    }

    /// Send a welcome email to a new user.
    pub async fn send_welcome(&self, to_email: &str) -> Result<String> {
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 40px 0;">
  <div style="max-width: 600px; margin: 0 auto; padding: 0 20px;">
    <h1 style="font-weight: normal; text-align: center;">Welcome to AllSource</h1>
    <p>Thanks for signing up! Your account is ready.</p>
    <p>AllSource is the event store built for real-time analytics and event sourcing.</p>
    <div style="margin: 32px 0;">
      <a href="{app_url}" style="background: #000; color: #fff; padding: 12px 24px; text-decoration: none; display: inline-block;">
        Get started
      </a>
    </div>
    <hr style="border: none; border-top: 1px solid #eee; margin: 24px 0;">
    <p style="color: #666; font-size: 12px;">AllSource</p>
  </div>
</body>
</html>"#,
            app_url = self.app_url
        );

        self.send(to_email, "Welcome to AllSource", &html).await
    }

    /// Send a password reset email.
    pub async fn send_password_reset(&self, to_email: &str, reset_url: &str) -> Result<String> {
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 40px 0;">
  <div style="max-width: 600px; margin: 0 auto; padding: 0 20px;">
    <h1 style="font-weight: normal; text-align: center;">Reset your password</h1>
    <p>We received a request to reset your password. Click the button below to choose a new one.</p>
    <div style="margin: 32px 0;">
      <a href="{reset_url}" style="background: #000; color: #fff; padding: 12px 24px; text-decoration: none; display: inline-block;">
        Reset password
      </a>
    </div>
    <p style="color: #666; font-size: 14px;">If you didn't request this, you can safely ignore this email.</p>
    <hr style="border: none; border-top: 1px solid #eee; margin: 24px 0;">
    <p style="color: #666; font-size: 12px;">AllSource</p>
  </div>
</body>
</html>"#,
            reset_url = reset_url
        );

        self.send(to_email, "Reset your password", &html).await
    }

    /// Send an email verification link.
    pub async fn send_verification(&self, to_email: &str, verify_url: &str) -> Result<String> {
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; margin: 0; padding: 40px 0;">
  <div style="max-width: 600px; margin: 0 auto; padding: 0 20px;">
    <h1 style="font-weight: normal; text-align: center;">Verify your email</h1>
    <p>Click the button below to verify your email address.</p>
    <div style="margin: 32px 0;">
      <a href="{verify_url}" style="background: #000; color: #fff; padding: 12px 24px; text-decoration: none; display: inline-block;">
        Verify email
      </a>
    </div>
    <p style="color: #666; font-size: 14px;">If you didn't create an account, you can safely ignore this email.</p>
    <hr style="border: none; border-top: 1px solid #eee; margin: 24px 0;">
    <p style="color: #666; font-size: 12px;">AllSource</p>
  </div>
</body>
</html>"#,
            verify_url = verify_url
        );

        self.send(to_email, "Verify your email", &html).await
    }

    /// Low-level send via Resend API.
    async fn send(&self, to: &str, subject: &str, html: &str) -> Result<String> {
        let request = SendEmailRequest {
            from: &self.from_email,
            to: vec![to],
            subject,
            html,
        };

        let response = self
            .client
            .post("https://api.resend.com/emails")
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .context("Failed to call Resend API")?;

        if response.status().is_success() {
            let result: SendEmailResponse = response.json().await?;
            info!(email_id = %result.id, to = to, subject = subject, "Email sent via Resend");
            Ok(result.id)
        } else {
            let status = response.status();
            let error: ResendError = response.json().await.unwrap_or(ResendError {
                message: "Unknown error".to_string(),
            });
            warn!(to = to, subject = subject, status = %status, error = %error.message, "Resend API error");
            Err(anyhow::anyhow!(
                "Resend API error ({}): {}",
                status,
                error.message
            ))
        }
    }
}

/// Placeholder that logs instead of sending (used when `RESEND_API_KEY` is not set).
#[derive(Clone)]
pub struct NoOpEmailService;

impl NoOpEmailService {
    pub async fn send_welcome(&self, to_email: &str) -> Result<String> {
        warn!(
            to = to_email,
            "Email not sent (no RESEND_API_KEY configured)"
        );
        Ok("noop".to_string())
    }

    pub async fn send_password_reset(&self, to_email: &str, _reset_url: &str) -> Result<String> {
        warn!(
            to = to_email,
            "Password reset email not sent (no RESEND_API_KEY configured)"
        );
        Ok("noop".to_string())
    }

    pub async fn send_verification(&self, to_email: &str, _verify_url: &str) -> Result<String> {
        warn!(
            to = to_email,
            "Verification email not sent (no RESEND_API_KEY configured)"
        );
        Ok("noop".to_string())
    }
}

/// Enum dispatch for email service (real or noop).
#[derive(Clone)]
pub enum Email {
    Resend(EmailService),
    NoOp(NoOpEmailService),
}

impl Email {
    /// Create from environment. Falls back to `NoOp` if `RESEND_API_KEY` is not set.
    pub fn from_env() -> Self {
        match EmailService::from_env() {
            Some(service) => {
                info!("Email service configured (Resend)");
                Self::Resend(service)
            }
            None => {
                warn!("RESEND_API_KEY not set -- emails will be logged but not sent");
                Self::NoOp(NoOpEmailService)
            }
        }
    }

    pub async fn send_welcome(&self, to_email: &str) -> Result<String> {
        match self {
            Self::Resend(s) => s.send_welcome(to_email).await,
            Self::NoOp(s) => s.send_welcome(to_email).await,
        }
    }

    pub async fn send_password_reset(&self, to_email: &str, reset_url: &str) -> Result<String> {
        match self {
            Self::Resend(s) => s.send_password_reset(to_email, reset_url).await,
            Self::NoOp(s) => s.send_password_reset(to_email, reset_url).await,
        }
    }

    pub async fn send_verification(&self, to_email: &str, verify_url: &str) -> Result<String> {
        match self {
            Self::Resend(s) => s.send_verification(to_email, verify_url).await,
            Self::NoOp(s) => s.send_verification(to_email, verify_url).await,
        }
    }
}
