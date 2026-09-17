//! Async webhook delivery worker.
//!
//! Receives webhook delivery tasks from the event ingestion path via an mpsc channel,
//! and delivers events to registered webhook URLs with retry and exponential backoff.

use crate::{
    application::services::webhook::{
        DeliveryStatus, WebhookDelivery, WebhookRegistry, WebhookSubscription,
    },
    domain::entities::Event,
    store::WebhookDeliveryTask,
};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Maximum number of delivery attempts per event
const MAX_ATTEMPTS: u32 = 5;

/// Base delay for exponential backoff (seconds)
const BASE_BACKOFF_SECS: u64 = 2;

/// Run the webhook delivery worker.
///
/// Consumes tasks from the channel and delivers them asynchronously.
/// Each delivery is spawned as an independent tokio task for parallelism.
#[cfg_attr(feature = "hotpath", hotpath::measure)]
pub async fn run_webhook_delivery_worker(
    mut rx: mpsc::UnboundedReceiver<WebhookDeliveryTask>,
    registry: Arc<WebhookRegistry>,
) {
    tracing::info!("Webhook delivery worker started");

    while let Some(task) = rx.recv().await {
        let registry = Arc::clone(&registry);
        tokio::spawn(async move {
            deliver_with_retry(&task.webhook, &task.event, &registry).await;
        });
    }

    tracing::info!("Webhook delivery worker stopped (channel closed)");
}

/// Deliver an event to a webhook URL with exponential backoff retry.
async fn deliver_with_retry(
    webhook: &WebhookSubscription,
    event: &Event,
    registry: &WebhookRegistry,
) {
    let delivery_id = Uuid::new_v4();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    // Build the payload
    let payload = build_payload(webhook, event);

    // Compute HMAC-SHA256 signature
    let signature = compute_signature(&webhook.secret, &payload);

    for attempt in 1..=MAX_ATTEMPTS {
        let result = client
            .post(&webhook.url)
            .header("Content-Type", "application/json")
            .header("X-AllSource-Signature", &signature)
            .header("X-AllSource-Event-Type", event.event_type_str())
            .header("X-AllSource-Delivery-Id", delivery_id.to_string())
            .header("X-AllSource-Webhook-Id", webhook.id.to_string())
            .body(payload.clone())
            .send()
            .await;

        match result {
            Ok(response) => {
                let status_code = response.status().as_u16();
                let body = response.text().await.unwrap_or_else(|_| String::new());

                if (200..300).contains(&status_code) {
                    // Success
                    registry.record_delivery(WebhookDelivery {
                        id: delivery_id,
                        webhook_id: webhook.id,
                        event_id: event.id,
                        status: DeliveryStatus::Success,
                        attempt,
                        max_attempts: MAX_ATTEMPTS,
                        response_status: Some(status_code),
                        response_body: if body.is_empty() {
                            None
                        } else {
                            Some(truncate_string(&body, 500))
                        },
                        error: None,
                        created_at: Utc::now(),
                        next_retry_at: None,
                    });

                    tracing::debug!(
                        "Webhook delivered: {} -> {} (attempt {})",
                        event.id,
                        webhook.url,
                        attempt
                    );
                    return;
                }
                // Non-2xx response — retry
                let delay = backoff_delay(attempt);
                tracing::warn!(
                    "Webhook delivery failed: {} -> {} (status {}, attempt {}/{}), retrying in {}s",
                    event.id,
                    webhook.url,
                    status_code,
                    attempt,
                    MAX_ATTEMPTS,
                    delay.as_secs()
                );

                registry.record_delivery(WebhookDelivery {
                    id: delivery_id,
                    webhook_id: webhook.id,
                    event_id: event.id,
                    status: if attempt < MAX_ATTEMPTS {
                        DeliveryStatus::Retrying
                    } else {
                        DeliveryStatus::Failed
                    },
                    attempt,
                    max_attempts: MAX_ATTEMPTS,
                    response_status: Some(status_code),
                    response_body: Some(truncate_string(&body, 500)),
                    error: None,
                    created_at: Utc::now(),
                    next_retry_at: if attempt < MAX_ATTEMPTS {
                        Some(Utc::now() + chrono::Duration::seconds(delay.as_secs() as i64))
                    } else {
                        None
                    },
                });

                if attempt < MAX_ATTEMPTS {
                    tokio::time::sleep(delay).await;
                }
            }
            Err(e) => {
                // Connection/timeout error — retry
                let delay = backoff_delay(attempt);
                tracing::warn!(
                    "Webhook delivery error: {} -> {} ({}, attempt {}/{}), retrying in {}s",
                    event.id,
                    webhook.url,
                    e,
                    attempt,
                    MAX_ATTEMPTS,
                    delay.as_secs()
                );

                registry.record_delivery(WebhookDelivery {
                    id: delivery_id,
                    webhook_id: webhook.id,
                    event_id: event.id,
                    status: if attempt < MAX_ATTEMPTS {
                        DeliveryStatus::Retrying
                    } else {
                        DeliveryStatus::Failed
                    },
                    attempt,
                    max_attempts: MAX_ATTEMPTS,
                    response_status: None,
                    response_body: None,
                    error: Some(e.to_string()),
                    created_at: Utc::now(),
                    next_retry_at: if attempt < MAX_ATTEMPTS {
                        Some(Utc::now() + chrono::Duration::seconds(delay.as_secs() as i64))
                    } else {
                        None
                    },
                });

                if attempt < MAX_ATTEMPTS {
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    tracing::error!(
        "Webhook delivery permanently failed after {} attempts: {} -> {}",
        MAX_ATTEMPTS,
        event.id,
        webhook.url
    );
}

/// Build the JSON payload for webhook delivery
fn build_payload(webhook: &WebhookSubscription, event: &Event) -> String {
    let payload = serde_json::json!({
        "webhook_id": webhook.id,
        "event": {
            "id": event.id,
            "event_type": event.event_type_str(),
            "entity_id": event.entity_id_str(),
            "timestamp": event.timestamp,
            "payload": event.payload,
            "metadata": event.metadata,
        },
        "delivered_at": Utc::now(),
    });
    serde_json::to_string(&payload).unwrap_or_default()
}

/// Compute HMAC-SHA256 signature for webhook payload verification
fn compute_signature(secret: &str, payload: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    let code_bytes = result.into_bytes();

    // Return as hex string with sha256= prefix
    format!("sha256={}", hex::encode(code_bytes))
}

/// Calculate exponential backoff delay: base * 2^(attempt-1)
fn backoff_delay(attempt: u32) -> std::time::Duration {
    let secs = BASE_BACKOFF_SECS * 2u64.pow(attempt.saturating_sub(1));
    // Cap at 5 minutes
    std::time::Duration::from_secs(secs.min(300))
}

/// Truncate a string to a maximum length
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_delay() {
        assert_eq!(backoff_delay(1), std::time::Duration::from_secs(2));
        assert_eq!(backoff_delay(2), std::time::Duration::from_secs(4));
        assert_eq!(backoff_delay(3), std::time::Duration::from_secs(8));
        assert_eq!(backoff_delay(4), std::time::Duration::from_secs(16));
        assert_eq!(backoff_delay(5), std::time::Duration::from_secs(32));
    }

    #[test]
    fn test_backoff_delay_capped() {
        // Very high attempt should be capped at 300s
        assert_eq!(backoff_delay(20), std::time::Duration::from_mins(5));
    }

    #[test]
    fn test_compute_signature() {
        let sig = compute_signature("test-secret", r#"{"test": true}"#);
        assert!(sig.starts_with("sha256="));
        assert_eq!(sig.len(), 7 + 64); // "sha256=" + 64 hex chars
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("short", 10), "short");
        assert_eq!(truncate_string("a longer string", 5), "a lon...");
    }

    #[test]
    fn test_build_payload() {
        let webhook = WebhookSubscription {
            id: Uuid::new_v4(),
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/hook".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: "secret".to_string(),
            active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            description: None,
        };

        let event = Event::from_strings(
            "user.created".to_string(),
            "user-1".to_string(),
            "default".to_string(),
            serde_json::json!({"name": "Test"}),
            None,
        )
        .unwrap();

        let payload = build_payload(&webhook, &event);
        let parsed: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(parsed["event"]["event_type"], "user.created");
        assert_eq!(parsed["event"]["entity_id"], "user-1");
        assert_eq!(parsed["webhook_id"], webhook.id.to_string());
    }
}
