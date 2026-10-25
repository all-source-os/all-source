use crate::infrastructure::observability::metrics::MetricsRegistry;
use axum::{
    body::HttpBody,
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::Response,
};
use std::{sync::Arc, time::Instant};

/// Records request count, duration and response body bytes per route template.
/// Layer it between [`super::response_limit::bounded`] and compression: the
/// buffered body then has an exact size, and that size is the uncompressed one.
pub async fn record(
    State(metrics): State<Arc<MetricsRegistry>>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_owned();
    let endpoint = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str)
        .to_owned();
    metrics.http_requests_in_flight.inc();
    let started = Instant::now();
    let response = next.run(request).await;
    metrics.http_requests_in_flight.dec();
    let status = response.status().as_str().to_owned();
    let labels = [method.as_str(), endpoint.as_str(), status.as_str()];
    metrics.http_requests_total.with_label_values(&labels).inc();
    metrics
        .http_request_duration_seconds
        .with_label_values(&[method.as_str(), endpoint.as_str()])
        .observe(started.elapsed().as_secs_f64());
    if let Some(bytes) = response.body().size_hint().exact() {
        metrics
            .http_response_bytes_total
            .with_label_values(&labels)
            .inc_by(bytes);
    }
    response
}
