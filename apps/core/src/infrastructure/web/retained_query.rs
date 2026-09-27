//! Explicit wire contract for callers that cannot use tolerant archive reads.
use super::archive_work;
use crate::{
    application::dto::{
        EventDto, QueryEventsRequest, QueryEventsResponse, RetainedEntityIntegrity,
    },
    domain::value_objects::{EntityId, TenantId},
    error::{AllSourceError, Result},
    store::EventStore,
};
use std::sync::Arc;

pub(super) async fn query(
    store: Arc<EventStore>,
    request: QueryEventsRequest,
    offset: usize,
    descending: bool,
    protocol: &str,
) -> Result<QueryEventsResponse> {
    if protocol != "retained-entity-v1"
        || offset != 0
        || descending
        || request.event_type.is_some()
        || request.event_type_prefix.is_some()
        || request.exclude_event_type_prefix.is_some()
        || request.payload_filter.is_some()
        || request.since.is_some()
        || request.until.is_some()
        || request.as_of.is_some()
    {
        return Err(AllSourceError::InvalidInput(
            "Strict retained read requires retained-entity-v1 without filters, offset or descending order"
                .into(),
        ));
    }
    let (Some(tenant), Some(entity), Some(limit @ 1..=1001)) =
        (request.tenant_id, request.entity_id, request.limit)
    else {
        return Err(AllSourceError::InvalidInput(
            "Strict retained read requires tenant_id, entity_id and limit 1..1001".into(),
        ));
    };
    TenantId::new(tenant.clone())?;
    EntityId::new(entity.clone())?;
    let (events, total_count) =
        archive_work::retained_query(store, tenant.clone(), entity.clone(), limit).await?;
    let count = events.len();
    Ok(QueryEventsResponse {
        events: events.into_iter().map(EventDto::from).collect(),
        count,
        total_count,
        has_more: count < total_count,
        entity_version: None,
        archive_integrity: Some(RetainedEntityIntegrity {
            protocol: "retained-entity-v1",
            tenant_id: tenant,
            entity_id: entity,
        }),
    })
}
