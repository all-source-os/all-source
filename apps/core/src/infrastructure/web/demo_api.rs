use crate::{
    application::dto::QueryEventsRequest, domain::entities::Event, error::Result,
    infrastructure::web::api_v1::AppState,
};
use axum::{Json, extract::State};
use rand::RngExt;

/// Marker event type used to detect whether seeding has already occurred.
const DEMO_SEED_MARKER: &str = "demo.seed_marker";

/// Total number of demo events to seed.
const DEMO_EVENT_COUNT: usize = 1000;

/// Embedding dimension for synthetic vectors (MiniLM-compatible).
const EMBEDDING_DIM: usize = 384;

/// Seed demo data into the event store.
///
/// Idempotent: if a `demo.seed_marker` event already exists, returns
/// immediately without duplicating data.
pub async fn demo_seed_handler(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    // Idempotency check: look for our marker event
    let existing = state.store.query(&QueryEventsRequest {
        as_of_valid: None,
        since_valid: None,
        until_valid: None,
        entity_id: None,
        event_type: Some(DEMO_SEED_MARKER.to_string()),
        tenant_id: None,
        as_of: None,
        since: None,
        until: None,
        limit: Some(1),
        event_type_prefix: None,
        exclude_event_type_prefix: None,
        payload_filter: None,
    })?;

    if !existing.is_empty() {
        return Ok(Json(serde_json::json!({
            "seeded": true,
            "event_count": DEMO_EVENT_COUNT,
            "message": "Demo data already seeded"
        })));
    }

    let mut rng = rand::rng();

    // Event type definitions with their categories and counts
    struct EventSpec {
        event_type: &'static str,
        entity_prefix: &'static str,
        count: usize,
        category_index: usize,
    }

    let specs = [
        EventSpec {
            event_type: "log.info",
            entity_prefix: "server",
            count: 200,
            category_index: 0,
        },
        EventSpec {
            event_type: "log.warning",
            entity_prefix: "server",
            count: 100,
            category_index: 1,
        },
        EventSpec {
            event_type: "log.error",
            entity_prefix: "server",
            count: 100,
            category_index: 2,
        },
        EventSpec {
            event_type: "metric.cpu",
            entity_prefix: "infra",
            count: 100,
            category_index: 3,
        },
        EventSpec {
            event_type: "metric.memory",
            entity_prefix: "infra",
            count: 100,
            category_index: 4,
        },
        EventSpec {
            event_type: "metric.latency",
            entity_prefix: "infra",
            count: 100,
            category_index: 5,
        },
        EventSpec {
            event_type: "user.signup",
            entity_prefix: "user",
            count: 80,
            category_index: 6,
        },
        EventSpec {
            event_type: "user.login",
            entity_prefix: "user",
            count: 80,
            category_index: 7,
        },
        EventSpec {
            event_type: "user.action",
            entity_prefix: "user",
            count: 80,
            category_index: 8,
        },
        EventSpec {
            event_type: "user.search",
            entity_prefix: "user",
            count: 60,
            category_index: 9,
        },
    ];

    let mut total_count: usize = 0;

    for spec in &specs {
        for i in 0..spec.count {
            let entity_id = format!("{}-{:04}", spec.entity_prefix, total_count);
            let payload = build_payload(spec.event_type, &mut rng, i);

            // Generate synthetic embedding vector (clustered by event type)
            let embedding: Vec<f32> = (0..EMBEDDING_DIM)
                .map(|d| {
                    let base = (spec.category_index as f32) * 0.1;
                    let noise: f32 = rng.random_range(-0.05..0.05);
                    ((d as f32 * 0.01 + base).sin() + noise).clamp(-1.0, 1.0)
                })
                .collect();

            // Merge embedding into payload
            let mut full_payload = payload;
            if let Some(obj) = full_payload.as_object_mut() {
                obj.insert("_embedding".to_string(), serde_json::json!(embedding));
            }

            let event = Event::from_strings(
                spec.event_type.to_string(),
                entity_id,
                "default".to_string(),
                full_payload,
                Some(serde_json::json!({
                    "source": "demo_seed",
                    "demo_index": total_count,
                })),
            )?;

            state.store.ingest(&event)?;
            total_count += 1;
        }
    }

    // Ingest the idempotency marker event last
    let marker = Event::from_strings(
        DEMO_SEED_MARKER.to_string(),
        "demo-seed-marker".to_string(),
        "default".to_string(),
        serde_json::json!({
            "event_count": total_count,
            "event_types": specs.iter().map(|s| s.event_type).collect::<Vec<_>>(),
        }),
        Some(serde_json::json!({ "source": "demo_seed" })),
    )?;
    state.store.ingest(&marker)?;

    tracing::info!("Demo data seeded: {} events", total_count);

    Ok(Json(serde_json::json!({
        "seeded": true,
        "event_count": total_count,
    })))
}

fn build_payload(event_type: &str, rng: &mut rand::rngs::ThreadRng, i: usize) -> serde_json::Value {
    match event_type {
        "log.info" => {
            let messages = [
                "Request processed successfully",
                "Cache hit for user session",
                "Database connection pool refreshed",
                "Background job completed",
                "Health check passed",
            ];
            serde_json::json!({
                "level": "info",
                "message": messages[i % messages.len()],
                "service": format!("api-{}", rng.random_range(1..=4)),
                "request_id": format!("req-{:08x}", rng.random::<u32>()),
                "duration_ms": rng.random_range(1..500),
            })
        }
        "log.warning" => {
            let messages = [
                "High memory usage detected",
                "Slow query detected (>1s)",
                "Rate limit threshold approaching",
                "Certificate expires in 30 days",
            ];
            let idx = rng.random_range(0..messages.len());
            serde_json::json!({
                "level": "warning",
                "message": messages[idx],
                "service": format!("api-{}", rng.random_range(1..=4)),
                "threshold": rng.random_range(70..95),
            })
        }
        "log.error" => {
            let messages = [
                "Connection timeout to upstream",
                "Failed to parse request body",
                "Authentication token expired",
                "Disk space critically low",
            ];
            let idx = rng.random_range(0..messages.len());
            serde_json::json!({
                "level": "error",
                "message": messages[idx],
                "service": format!("api-{}", rng.random_range(1..=4)),
                "error_code": rng.random_range(400..600),
                "stack_trace": "at handler.rs:42\n  at router.rs:108",
            })
        }
        "metric.cpu" => {
            serde_json::json!({
                "host": format!("node-{}", rng.random_range(1..=8)),
                "cpu_percent": rng.random_range(5.0_f64..99.0),
                "cores": 8,
                "load_avg_1m": rng.random_range(0.5_f64..12.0),
            })
        }
        "metric.memory" => {
            let total_gb: f64 = 32.0;
            let used: f64 = rng.random_range(8.0..30.0);
            serde_json::json!({
                "host": format!("node-{}", rng.random_range(1..=8)),
                "total_gb": total_gb,
                "used_gb": (used * 100.0).round() / 100.0,
                "percent": ((used / total_gb) * 100.0).round(),
            })
        }
        "metric.latency" => {
            let endpoints = ["/api/users", "/api/events", "/api/search", "/api/health"];
            let idx = rng.random_range(0..endpoints.len());
            serde_json::json!({
                "endpoint": endpoints[idx],
                "p50_ms": rng.random_range(5.0_f64..50.0),
                "p95_ms": rng.random_range(50.0_f64..200.0),
                "p99_ms": rng.random_range(200.0_f64..1000.0),
                "requests": rng.random_range(100..10000),
            })
        }
        "user.signup" => {
            let plans = ["free", "starter", "pro", "enterprise"];
            let sources = ["organic", "referral", "ad_campaign", "github"];
            let plan_idx = rng.random_range(0..plans.len());
            let source_idx = rng.random_range(0..sources.len());
            serde_json::json!({
                "user_id": format!("usr-{:06}", i),
                "email": format!("user{}@example.com", i),
                "plan": plans[plan_idx],
                "source": sources[source_idx],
            })
        }
        "user.login" => {
            let methods = ["password", "oauth_google", "oauth_github", "sso"];
            let method_idx = rng.random_range(0..methods.len());
            serde_json::json!({
                "user_id": format!("usr-{:06}", rng.random_range(0..500)),
                "method": methods[method_idx],
                "ip": format!("{}.{}.{}.{}", rng.random_range(10..200), rng.random_range(0..255), rng.random_range(0..255), rng.random_range(1..255)),
                "user_agent": "Mozilla/5.0",
            })
        }
        "user.action" => {
            let actions = [
                "viewed_dashboard",
                "created_event",
                "ran_query",
                "exported_data",
                "invited_member",
            ];
            let action_idx = rng.random_range(0..actions.len());
            let success = rng.random_range(0..10) > 1;
            serde_json::json!({
                "user_id": format!("usr-{:06}", rng.random_range(0..500)),
                "action": actions[action_idx],
                "duration_s": rng.random_range(1..300),
                "success": success,
            })
        }
        "user.search" => {
            let queries = [
                "cat videos",
                "error 500",
                "production deploy",
                "user signup",
                "latency spike",
                "memory leak",
                "api timeout",
                "database slow",
            ];
            let query_idx = rng.random_range(0..queries.len());
            serde_json::json!({
                "user_id": format!("usr-{:06}", rng.random_range(0..500)),
                "query": queries[query_idx],
                "results_count": rng.random_range(0..100),
                "took_ms": rng.random_range(5..200),
            })
        }
        _ => serde_json::json!({}),
    }
}
