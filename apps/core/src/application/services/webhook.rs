use crate::{
    domain::entities::Event,
    infrastructure::memory::ledger::{
        MapCharge, Owner, Tally, option_string_heap, string_heap, strings_heap,
    },
};
use chrono::{DateTime, Utc};
use dashmap::{DashMap, mapref::entry::Entry};
use serde::{Deserialize, Serialize};
use std::mem::size_of;
use uuid::Uuid;

/// A registered webhook subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSubscription {
    /// Unique ID for this subscription
    pub id: Uuid,
    /// Tenant that owns this webhook
    pub tenant_id: String,
    /// URL to deliver events to
    pub url: String,
    /// Optional: only deliver events matching these event types (glob patterns)
    /// If empty, all events are delivered
    pub event_types: Vec<String>,
    /// Optional: only deliver events matching these entity IDs
    pub entity_ids: Vec<String>,
    /// Secret used to sign webhook payloads (HMAC-SHA256)
    pub secret: String,
    /// Whether this webhook is currently active
    pub active: bool,
    /// When this webhook was created
    pub created_at: DateTime<Utc>,
    /// When this webhook was last updated
    pub updated_at: DateTime<Utc>,
    /// Optional description
    pub description: Option<String>,
}

/// Request to register a new webhook
#[derive(Debug, Clone, Deserialize)]
pub struct RegisterWebhookRequest {
    pub tenant_id: String,
    pub url: String,
    #[serde(default)]
    pub event_types: Vec<String>,
    #[serde(default)]
    pub entity_ids: Vec<String>,
    pub secret: Option<String>,
    pub description: Option<String>,
}

/// Request to update an existing webhook
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateWebhookRequest {
    pub url: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub entity_ids: Option<Vec<String>>,
    pub active: Option<bool>,
    pub description: Option<String>,
}

/// A webhook delivery attempt record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub webhook_id: Uuid,
    pub event_id: Uuid,
    pub status: DeliveryStatus,
    pub attempt: u32,
    pub max_attempts: u32,
    pub response_status: Option<u16>,
    pub response_body: Option<String>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub next_retry_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryStatus {
    Pending,
    Success,
    Failed,
    Retrying,
}

/// Owned heap of a subscription: its strings and vectors, not the inline value.
fn subscription_heap(webhook: &WebhookSubscription) -> u64 {
    string_heap(&webhook.tenant_id)
        + string_heap(&webhook.url)
        + strings_heap(&webhook.event_types)
        + strings_heap(&webhook.entity_ids)
        + string_heap(&webhook.secret)
        + option_string_heap(&webhook.description)
}

fn delivery_heap(delivery: &WebhookDelivery) -> u64 {
    option_string_heap(&delivery.response_body) + option_string_heap(&delivery.error)
}

/// Owned heap of a delivery log: its buffer and each record's strings.
fn deliveries_heap(deliveries: &Vec<WebhookDelivery>) -> u64 {
    (deliveries.capacity() * size_of::<WebhookDelivery>()) as u64
        + deliveries.iter().map(delivery_heap).sum::<u64>()
}

/// Manages webhook subscriptions in-memory using DashMap
pub struct WebhookRegistry {
    /// Webhooks indexed by ID
    webhooks: DashMap<Uuid, WebhookSubscription>,
    webhooks_charge: MapCharge,
    /// Index: tenant_id -> list of webhook IDs
    tenant_index: DashMap<String, Vec<Uuid>>,
    tenant_index_charge: MapCharge,
    /// Delivery log (webhook_id -> deliveries)
    deliveries: DashMap<Uuid, Vec<WebhookDelivery>>,
    deliveries_charge: MapCharge,
    /// Buffers of the `tenant_index` and `deliveries` vectors, and the
    /// tenant-key strings, tracked as their capacities change.
    vec_slots: Tally,
}

impl Default for WebhookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WebhookRegistry {
    pub fn new() -> Self {
        Self {
            webhooks: DashMap::new(),
            webhooks_charge: MapCharge::new(Owner::Misc, size_of::<(Uuid, WebhookSubscription)>()),
            tenant_index: DashMap::new(),
            tenant_index_charge: MapCharge::new(Owner::Misc, size_of::<(String, Vec<Uuid>)>()),
            deliveries: DashMap::new(),
            deliveries_charge: MapCharge::new(
                Owner::Misc,
                size_of::<(Uuid, Vec<WebhookDelivery>)>(),
            ),
            vec_slots: Tally::new(Owner::Misc),
        }
    }

    fn push_tenant_id(&self, tenant_id: String, id: Uuid) {
        match self.tenant_index.entry(tenant_id) {
            Entry::Occupied(mut slot) => self.push_id(slot.get_mut(), id),
            Entry::Vacant(slot) => {
                self.vec_slots.add(string_heap(slot.key()));
                self.push_id(&mut slot.insert(Vec::new()), id);
            }
        }
        self.tenant_index_charge.resize(&self.tenant_index);
    }

    fn push_id(&self, ids: &mut Vec<Uuid>, id: Uuid) {
        let before = ids.capacity();
        ids.push(id);
        self.vec_slots
            .grow(before, ids.capacity(), size_of::<Uuid>());
    }

    /// Register a new webhook subscription
    pub fn register(&self, req: RegisterWebhookRequest) -> WebhookSubscription {
        let now = Utc::now();
        let secret = req.secret.unwrap_or_else(|| {
            // Generate a random secret if none provided
            format!("whsec_{}", Uuid::new_v4().to_string().replace('-', ""))
        });

        let webhook = WebhookSubscription {
            id: Uuid::new_v4(),
            tenant_id: req.tenant_id.clone(),
            url: req.url,
            event_types: req.event_types,
            entity_ids: req.entity_ids,
            secret,
            active: true,
            created_at: now,
            updated_at: now,
            description: req.description,
        };

        let id = webhook.id;
        self.webhooks_charge
            .insert(&self.webhooks, id, webhook.clone(), |_, w| {
                subscription_heap(w)
            });

        self.push_tenant_id(req.tenant_id, id);

        webhook
    }

    /// Get a webhook by ID
    pub fn get(&self, id: Uuid) -> Option<WebhookSubscription> {
        self.webhooks.get(&id).map(|w| w.clone())
    }

    /// List webhooks for a tenant
    pub fn list_by_tenant(&self, tenant_id: &str) -> Vec<WebhookSubscription> {
        self.tenant_index
            .get(tenant_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.webhooks.get(id).map(|w| w.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Update a webhook
    pub fn update(&self, id: Uuid, req: UpdateWebhookRequest) -> Option<WebhookSubscription> {
        let mut webhook = self.webhooks.get_mut(&id)?;
        let w = webhook.value_mut();
        let heap_before = subscription_heap(w);

        if let Some(url) = req.url {
            w.url = url;
        }
        if let Some(event_types) = req.event_types {
            w.event_types = event_types;
        }
        if let Some(entity_ids) = req.entity_ids {
            w.entity_ids = entity_ids;
        }
        if let Some(active) = req.active {
            w.active = active;
        }
        if let Some(description) = req.description {
            w.description = Some(description);
        }
        w.updated_at = Utc::now();
        self.webhooks_charge
            .adjust_owned(heap_before, subscription_heap(w));

        Some(w.clone())
    }

    /// Delete a webhook
    pub fn delete(&self, id: Uuid) -> Option<WebhookSubscription> {
        let webhook = self
            .webhooks_charge
            .remove_entry(&self.webhooks, &id, |_, w| subscription_heap(w))?;

        // Remove from tenant index
        if let Some(mut ids) = self.tenant_index.get_mut(&webhook.tenant_id) {
            ids.retain(|wid| *wid != id);
        }

        // Clean up deliveries
        if let Some((_, deliveries)) = self.deliveries.remove(&id) {
            self.vec_slots.sub(deliveries_heap(&deliveries));
        }
        self.deliveries_charge.resize(&self.deliveries);

        Some(webhook)
    }

    /// Find all active webhooks that match a given event
    pub fn find_matching(&self, event: &Event) -> Vec<WebhookSubscription> {
        // Get all webhooks (in a real multi-tenant setup, we'd filter by tenant_id
        // from the event, but Core events currently use "default" tenant)
        let mut matching = Vec::new();

        for entry in &self.webhooks {
            let webhook = entry.value();
            if !webhook.active {
                continue;
            }

            // Check event type filter
            if !webhook.event_types.is_empty()
                && !webhook
                    .event_types
                    .iter()
                    .any(|pattern| matches_pattern(pattern, event.event_type_str()))
            {
                continue;
            }

            // Check entity ID filter
            if !webhook.entity_ids.is_empty()
                && !webhook
                    .entity_ids
                    .contains(&event.entity_id_str().to_string())
            {
                continue;
            }

            matching.push(webhook.clone());
        }

        matching
    }

    /// Record a delivery attempt
    pub fn record_delivery(&self, delivery: WebhookDelivery) {
        let webhook_id = delivery.webhook_id;
        self.vec_slots.add(delivery_heap(&delivery));
        let mut log = self.deliveries.entry(webhook_id).or_default();
        let before = log.capacity();
        log.push(delivery);
        self.vec_slots
            .grow(before, log.capacity(), size_of::<WebhookDelivery>());
        drop(log);
        self.deliveries_charge.resize(&self.deliveries);
    }

    /// Get delivery history for a webhook
    pub fn get_deliveries(&self, webhook_id: Uuid, limit: usize) -> Vec<WebhookDelivery> {
        self.deliveries
            .get(&webhook_id)
            .map(|deliveries| {
                let d = deliveries.value();
                let start = d.len().saturating_sub(limit);
                d[start..].to_vec()
            })
            .unwrap_or_default()
    }
}

/// Simple glob-style pattern matching for event types
/// Supports:
/// - exact match: "user.created" matches "user.created"
/// - wildcard suffix: "user.*" matches "user.created", "user.updated"
/// - full wildcard: "*" matches everything
fn matches_pattern(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return value.starts_with(prefix) && value[prefix.len()..].starts_with('.');
    }
    pattern == value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_event(entity_id: &str, event_type: &str) -> Event {
        Event::from_strings(
            event_type.to_string(),
            entity_id.to_string(),
            "default".to_string(),
            serde_json::json!({"test": true}),
            None,
        )
        .unwrap()
    }

    #[test]
    fn test_register_webhook() {
        let registry = WebhookRegistry::new();

        let webhook = registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/webhook".to_string(),
            event_types: vec!["user.*".to_string()],
            entity_ids: vec![],
            secret: Some("test-secret".to_string()),
            description: Some("Test webhook".to_string()),
        });

        assert_eq!(webhook.url, "https://example.com/webhook");
        assert_eq!(webhook.tenant_id, "tenant-1");
        assert!(webhook.active);
        assert_eq!(webhook.secret, "test-secret");
    }

    #[test]
    fn test_get_webhook() {
        let registry = WebhookRegistry::new();

        let webhook = registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/webhook".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        let found = registry.get(webhook.id).unwrap();
        assert_eq!(found.id, webhook.id);
        assert_eq!(found.url, webhook.url);
    }

    #[test]
    fn test_list_by_tenant() {
        let registry = WebhookRegistry::new();

        registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/hook1".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/hook2".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-2".to_string(),
            url: "https://other.com/hook".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        let tenant1_hooks = registry.list_by_tenant("tenant-1");
        assert_eq!(tenant1_hooks.len(), 2);

        let tenant2_hooks = registry.list_by_tenant("tenant-2");
        assert_eq!(tenant2_hooks.len(), 1);
    }

    #[test]
    fn test_update_webhook() {
        let registry = WebhookRegistry::new();

        let webhook = registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/webhook".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        let updated = registry
            .update(
                webhook.id,
                UpdateWebhookRequest {
                    url: Some("https://example.com/new-url".to_string()),
                    event_types: Some(vec!["order.*".to_string()]),
                    entity_ids: None,
                    active: Some(false),
                    description: Some("Updated".to_string()),
                },
            )
            .unwrap();

        assert_eq!(updated.url, "https://example.com/new-url");
        assert_eq!(updated.event_types, vec!["order.*".to_string()]);
        assert!(!updated.active);
        assert_eq!(updated.description, Some("Updated".to_string()));
    }

    #[test]
    fn test_delete_webhook() {
        let registry = WebhookRegistry::new();

        let webhook = registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/webhook".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        let deleted = registry.delete(webhook.id);
        assert!(deleted.is_some());
        assert!(registry.get(webhook.id).is_none());
        assert!(registry.list_by_tenant("tenant-1").is_empty());
    }

    #[test]
    fn test_find_matching_all_events() {
        let registry = WebhookRegistry::new();

        registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/all".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        let event = create_test_event("entity-1", "user.created");
        let matching = registry.find_matching(&event);
        assert_eq!(matching.len(), 1);
    }

    #[test]
    fn test_find_matching_event_type_wildcard() {
        let registry = WebhookRegistry::new();

        registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/users".to_string(),
            event_types: vec!["user.*".to_string()],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        let user_event = create_test_event("entity-1", "user.created");
        assert_eq!(registry.find_matching(&user_event).len(), 1);

        let order_event = create_test_event("entity-1", "order.placed");
        assert_eq!(registry.find_matching(&order_event).len(), 0);
    }

    #[test]
    fn test_find_matching_entity_filter() {
        let registry = WebhookRegistry::new();

        registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/entity".to_string(),
            event_types: vec![],
            entity_ids: vec!["entity-1".to_string()],
            secret: None,
            description: None,
        });

        let matching_event = create_test_event("entity-1", "user.created");
        assert_eq!(registry.find_matching(&matching_event).len(), 1);

        let non_matching = create_test_event("entity-2", "user.created");
        assert_eq!(registry.find_matching(&non_matching).len(), 0);
    }

    #[test]
    fn test_find_matching_inactive_skipped() {
        let registry = WebhookRegistry::new();

        let webhook = registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/hook".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        // Deactivate
        registry.update(
            webhook.id,
            UpdateWebhookRequest {
                url: None,
                event_types: None,
                entity_ids: None,
                active: Some(false),
                description: None,
            },
        );

        let event = create_test_event("entity-1", "user.created");
        assert_eq!(registry.find_matching(&event).len(), 0);
    }

    #[test]
    fn test_matches_pattern() {
        assert!(matches_pattern("*", "anything"));
        assert!(matches_pattern("user.created", "user.created"));
        assert!(!matches_pattern("user.created", "user.updated"));
        assert!(matches_pattern("user.*", "user.created"));
        assert!(matches_pattern("user.*", "user.updated"));
        assert!(!matches_pattern("user.*", "order.placed"));
        assert!(!matches_pattern("user.*", "user"));
    }

    #[test]
    fn test_record_and_get_deliveries() {
        let registry = WebhookRegistry::new();
        let webhook_id = Uuid::new_v4();

        registry.record_delivery(WebhookDelivery {
            id: Uuid::new_v4(),
            webhook_id,
            event_id: Uuid::new_v4(),
            status: DeliveryStatus::Success,
            attempt: 1,
            max_attempts: 5,
            response_status: Some(200),
            response_body: None,
            error: None,
            created_at: Utc::now(),
            next_retry_at: None,
        });

        registry.record_delivery(WebhookDelivery {
            id: Uuid::new_v4(),
            webhook_id,
            event_id: Uuid::new_v4(),
            status: DeliveryStatus::Failed,
            attempt: 1,
            max_attempts: 5,
            response_status: Some(500),
            response_body: Some("Internal error".to_string()),
            error: None,
            created_at: Utc::now(),
            next_retry_at: None,
        });

        let deliveries = registry.get_deliveries(webhook_id, 10);
        assert_eq!(deliveries.len(), 2);
        assert_eq!(deliveries[0].status, DeliveryStatus::Success);
        assert_eq!(deliveries[1].status, DeliveryStatus::Failed);
    }

    #[test]
    fn test_auto_generated_secret() {
        let registry = WebhookRegistry::new();

        let webhook = registry.register(RegisterWebhookRequest {
            tenant_id: "tenant-1".to_string(),
            url: "https://example.com/hook".to_string(),
            event_types: vec![],
            entity_ids: vec![],
            secret: None,
            description: None,
        });

        assert!(webhook.secret.starts_with("whsec_"));
        assert!(webhook.secret.len() > 10);
    }
}
