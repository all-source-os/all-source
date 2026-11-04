use crate::types::Event;

/// Trait for reconstructing domain state from an event stream.
///
/// Implement this on your domain aggregate to fold raw events into typed state.
///
/// # Example
///
/// ```rust
/// use allsource::{Event, EventFolder};
///
/// #[derive(Default)]
/// struct OrderFolder {
///     total: f64,
///     items: Vec<String>,
///     status: String,
/// }
///
/// struct Order {
///     total: f64,
///     items: Vec<String>,
///     status: String,
/// }
///
/// impl EventFolder for OrderFolder {
///     type State = Order;
///
///     fn apply(&mut self, event: &Event) -> bool {
///         match event.event_type.as_str() {
///             "order.created" => {
///                 self.status = "created".into();
///                 true
///             }
///             "order.item_added" => {
///                 if let Some(item) = event.payload.get("item").and_then(|v| v.as_str()) {
///                     self.items.push(item.to_string());
///                 }
///                 if let Some(price) = event.payload.get("price").and_then(|v| v.as_f64()) {
///                     self.total += price;
///                 }
///                 true
///             }
///             "order.completed" => {
///                 self.status = "completed".into();
///                 true
///             }
///             _ => false,
///         }
///     }
///
///     fn finalize(self) -> Option<Self::State> {
///         if self.status.is_empty() {
///             None
///         } else {
///             Some(Order {
///                 total: self.total,
///                 items: self.items,
///                 status: self.status,
///             })
///         }
///     }
/// }
/// ```
pub trait EventFolder: Default {
    /// The domain state produced by folding events.
    type State;

    /// Apply a single event to the accumulator. Returns `true` if the event
    /// was relevant and applied, `false` if it was ignored.
    fn apply(&mut self, event: &Event) -> bool;

    /// Finalize the folder into domain state. Returns `None` if no relevant
    /// events were applied (e.g., entity doesn't exist).
    fn finalize(self) -> Option<Self::State>;
}

/// Fold a sequence of events into domain state using an [`EventFolder`].
///
/// Events should be ordered chronologically (oldest first).
pub fn fold_events<F: EventFolder>(events: &[Event]) -> Option<F::State> {
    let mut folder = F::default();
    for event in events {
        folder.apply(event);
    }
    folder.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Default)]
    struct Counter {
        count: u64,
        last_value: Option<String>,
    }

    struct CounterState {
        count: u64,
        last_value: String,
    }

    impl EventFolder for Counter {
        type State = CounterState;

        fn apply(&mut self, event: &Event) -> bool {
            match event.event_type.as_str() {
                "counter.incremented" => {
                    self.count += 1;
                    self.last_value = event
                        .payload
                        .get("value")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    true
                }
                _ => false,
            }
        }

        fn finalize(self) -> Option<Self::State> {
            self.last_value.map(|v| CounterState {
                count: self.count,
                last_value: v,
            })
        }
    }

    fn make_event(event_type: &str, payload: serde_json::Value) -> Event {
        Event {
            id: "test".into(),
            event_type: event_type.into(),
            entity_id: "entity-1".into(),
            payload,
            metadata: json!({}),
            timestamp: "2026-01-01T00:00:00Z".into(),
            version: Some(1),
            tenant_id: None,
            valid_from: None,
            valid_to: None,
        }
    }

    #[test]
    fn test_fold_events_basic() {
        let events = vec![
            make_event("counter.incremented", json!({"value": "a"})),
            make_event("counter.incremented", json!({"value": "b"})),
            make_event("unrelated.event", json!({})),
            make_event("counter.incremented", json!({"value": "c"})),
        ];

        let state = fold_events::<Counter>(&events).unwrap();
        assert_eq!(state.count, 3);
        assert_eq!(state.last_value, "c");
    }

    #[test]
    fn test_fold_events_empty() {
        let events: Vec<Event> = vec![];
        let state = fold_events::<Counter>(&events);
        assert!(state.is_none());
    }

    #[test]
    fn test_fold_events_no_relevant() {
        let events = vec![make_event("unrelated.event", json!({}))];
        let state = fold_events::<Counter>(&events);
        assert!(state.is_none());
    }
}
