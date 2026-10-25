//! Narrow archive reads for the event feed: number a tenant's history from
//! its id, type and entity columns, and decode payloads only for the rows a
//! page returns.
use super::*;
use arrow::array::StringArray;
use parquet::arrow::{
    ProjectionMask,
    arrow_reader::{ParquetRecordBatchReaderBuilder, RowSelection, RowSelector},
};

/// One archived event as the feed indexes it: enough to filter by type and
/// entity, and where to find the payload.
pub(crate) struct FeedKey {
    pub id: uuid::Uuid,
    pub event_type: String,
    pub entity_id: String,
    pub file: Arc<PathBuf>,
    pub row: u64,
}

fn string_column<'a>(batch: &'a RecordBatch, index: usize, name: &str) -> Result<&'a StringArray> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| AllSourceError::StorageError(format!("Invalid {name} column")))
}

impl ParquetStorage {
    /// Every archived event of the tenant in archive path order, reading only
    /// the event_id, event_type and entity_id columns.
    pub(crate) fn scan_feed_keys(
        &self,
        tenant_id: &str,
        budget: &mut ArchiveReadBudget,
    ) -> Result<Vec<FeedKey>> {
        let mut keys = Vec::new();
        for path in self.list_complete_tenant_archive(tenant_id, budget)? {
            budget.check()?;
            let file = File::open(&path).map_err(|error| unreadable(&path, error))?;
            budget.compressed(
                file.metadata()
                    .map_err(|error| unreadable(&path, error))?
                    .len(),
            )?;
            let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
            for group in builder.metadata().row_groups() {
                budget.rows(group.num_rows())?;
            }
            let mask = ProjectionMask::roots(builder.parquet_schema(), [0, 1, 2]);
            let path = Arc::new(path);
            let mut row = 0_u64;
            for batch in builder
                .with_projection(mask)
                .with_batch_size(4096)
                .build()?
            {
                budget.check()?;
                let batch = batch?;
                let ids = string_column(&batch, 0, "event_id")?;
                let types = string_column(&batch, 1, "event_type")?;
                let entities = string_column(&batch, 2, "entity_id")?;
                budget.decoded_payload(
                    (ids.value_data().len()
                        + types.value_data().len()
                        + entities.value_data().len()) as u64,
                )?;
                for index in 0..batch.num_rows() {
                    keys.push(FeedKey {
                        id: uuid::Uuid::parse_str(ids.value(index)).map_err(|e| {
                            AllSourceError::StorageError(format!("Invalid UUID: {e}"))
                        })?,
                        event_type: types.value(index).to_owned(),
                        entity_id: entities.value(index).to_owned(),
                        file: Arc::clone(&path),
                        row,
                    });
                    row += 1;
                }
            }
        }
        budget.check()?;
        Ok(keys)
    }

    /// Decode the events at `rows` (ascending, file-relative) of one archive
    /// file. Row groups holding none of them are not read.
    pub(crate) fn read_feed_rows(
        &self,
        path: &Path,
        rows: &[u64],
        tenant_id: &str,
        budget: &mut ArchiveReadBudget,
    ) -> Result<Vec<Event>> {
        budget.check()?;
        let file = File::open(path).map_err(|error| unreadable(path, error))?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
        let mut groups = Vec::new();
        let mut selectors = Vec::new();
        let mut wanted = rows.iter().copied().peekable();
        let mut group_start = 0_u64;
        for (index, group) in builder.metadata().row_groups().iter().enumerate() {
            let group_rows = u64::try_from(group.num_rows())
                .map_err(|_| AllSourceError::StorageError("Invalid row-group row count".into()))?;
            let group_end = group_start + group_rows;
            let mut cursor = group_start;
            let mut selected = false;
            while let Some(&row) = wanted.peek() {
                if row >= group_end {
                    break;
                }
                if row < cursor {
                    return Err(AllSourceError::StorageError(
                        "feed rows must be ascending and unique".into(),
                    ));
                }
                if row > cursor {
                    selectors.push(RowSelector::skip((row - cursor) as usize));
                }
                selectors.push(RowSelector::select(1));
                cursor = row + 1;
                selected = true;
                wanted.next();
            }
            if selected {
                if group_end > cursor {
                    selectors.push(RowSelector::skip((group_end - cursor) as usize));
                }
                groups.push(index);
            }
            group_start = group_end;
        }
        if wanted.next().is_some() {
            return Err(AllSourceError::StorageError(
                "feed row is past the end of its archive file".into(),
            ));
        }
        let mut events = Vec::with_capacity(rows.len());
        for batch in builder
            .with_row_groups(groups)
            .with_row_selection(RowSelection::from(selectors))
            .with_batch_size(256)
            .build()?
        {
            budget.check()?;
            let batch = batch?;
            budget.decoded_payload(decoded_text_bytes(&batch))?;
            events.extend(self.record_batch_to_events(&batch, tenant_id)?);
        }
        if events.len() != rows.len() {
            return Err(AllSourceError::StorageError(
                "archive file returned a different number of feed rows".into(),
            ));
        }
        Ok(events)
    }
}
