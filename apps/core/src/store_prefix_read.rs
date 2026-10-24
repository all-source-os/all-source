use super::EventStore;
use crate::{
    application::dto::QueryEventsRequest,
    domain::{entities::Event, value_objects::TenantId},
    error::{AllSourceError, Result},
    infrastructure::persistence::archive_budget::ArchiveReadBudget,
};
use std::{
    collections::HashSet,
    sync::{Arc, atomic::AtomicBool},
};

impl EventStore {
    pub(crate) fn query_retained_prefix(
        &self,
        request: &QueryEventsRequest,
        offset: usize,
        descending: bool,
        cancellation: Arc<AtomicBool>,
    ) -> Result<(Vec<Event>, usize)> {
        let tenant = request
            .tenant_id
            .as_deref()
            .ok_or_else(|| AllSourceError::InvalidInput("Prefix query requires tenant".into()))?;
        TenantId::new(tenant.to_owned())?;
        let prefix = request
            .event_type_prefix
            .as_deref()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                AllSourceError::InvalidInput("Prefix query requires nonempty prefix".into())
            })?;
        let mut budget = ArchiveReadBudget::new(self.strict_archive_limits.clone())
            .with_cancellation(Some(cancellation));
        let _resident = self
            .cache_residency_gate
            .try_read_for(budget.remaining()?)
            .ok_or_else(|| {
                AllSourceError::StorageError("Prefix query residency lock timed out".into())
            })?;
        let mut selected = Vec::new();
        let mut seen = HashSet::new();
        let mut remaining = 2 * 1024 * 1024;
        let mut retain = |event: &Event, work: &ArchiveReadBudget| -> Result<()> {
            work.check()?;
            if event.tenant_id_str() != tenant
                || !event.event_type_str().starts_with(prefix)
                || !self.apply_filters(event, request)
                || seen.contains(&event.id)
            {
                return Ok(());
            }
            if selected.len() >= 10_000 {
                return Err(AllSourceError::StorageError(
                    "Prefix query event budget exceeded".into(),
                ));
            }
            let mut encoded = super::strict_read::EncodedBudget { remaining, work };
            serde_json::to_writer(&mut encoded, event).map_err(|_| {
                work.check().err().unwrap_or_else(|| {
                    AllSourceError::StorageError("Prefix query encoded budget exceeded".into())
                })
            })?;
            remaining = encoded.remaining;
            seen.insert(event.id);
            selected.push(event.clone());
            Ok(())
        };
        if let Some(storage) = &self.storage {
            storage
                .try_read_for(budget.remaining()?)
                .ok_or_else(|| {
                    AllSourceError::StorageError("Prefix query storage lock timed out".into())
                })?
                .visit_retained_prefix(tenant, prefix, &mut budget, &mut retain)?;
        }
        let events = self
            .events
            .try_read_for(budget.remaining()?)
            .ok_or_else(|| {
                AllSourceError::StorageError("Prefix query event lock timed out".into())
            })?;
        for event in events.iter() {
            retain(event, &budget)?;
        }
        selected.sort_by(|left, right| {
            let order = left
                .timestamp
                .timestamp_micros()
                .cmp(&right.timestamp.timestamp_micros())
                .then_with(|| left.version.cmp(&right.version))
                .then_with(|| left.id.cmp(&right.id));
            if descending { order.reverse() } else { order }
        });
        budget.check()?;
        let count = selected.len();
        Ok((
            selected
                .into_iter()
                .skip(offset)
                .take(request.limit.unwrap_or(100))
                .collect(),
            count,
        ))
    }
}

#[cfg(test)]
#[path = "store_prefix_read_tests.rs"]
mod tests;
