//! Strict entity scans share archive enumeration while keeping unrelated history out of residency.
use super::*;
use arrow::{array::BooleanArray, compute::filter_record_batch};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

impl ParquetStorage {
    pub(crate) fn visit_retained_entity(
        &self,
        tenant_id: &str,
        entity_id: &str,
        budget: &mut ArchiveReadBudget,
        mut visit: impl FnMut(&Event, &ArchiveReadBudget) -> Result<()>,
    ) -> Result<()> {
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
            let groups = builder
                .metadata()
                .row_groups()
                .iter()
                .enumerate()
                .filter(|(_, group)| row_group_may_hold_entity(group, entity_id.as_bytes()))
                .map(|(index, _)| index)
                .collect();
            for batch in builder
                .with_row_groups(groups)
                .with_batch_size(256)
                .build()?
            {
                budget.check()?;
                let batch = batch?;
                let ids = batch
                    .column_by_name("entity_id")
                    .and_then(|column| column.as_any().downcast_ref::<arrow::array::StringArray>())
                    .ok_or_else(|| {
                        AllSourceError::StorageError("Invalid entity_id column".into())
                    })?;
                if ids.null_count() != 0 {
                    return Err(AllSourceError::StorageError(
                        "Null entity_id in retained archive".into(),
                    ));
                }
                let mask =
                    BooleanArray::from_iter(ids.iter().map(|id| Some(id == Some(entity_id))));
                let selected = filter_record_batch(&batch, &mask)?;
                // Charge retained payload before JSON decoding; unrelated rows never become Events.
                budget.decoded_payload(decoded_text_bytes(&selected))?;
                for event in self.record_batch_to_events(&selected, tenant_id)? {
                    visit(&event, budget)?;
                }
            }
        }
        budget.check()
    }
}
