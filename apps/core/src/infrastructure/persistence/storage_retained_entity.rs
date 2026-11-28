//! Strict selections share archive enumeration while keeping unrelated history out of residency.
use super::*;
use arrow::{array::BooleanArray, compute::filter_record_batch};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

#[derive(Clone, Copy)]
enum Selection<'a> {
    Entity(&'a str),
    Prefix(&'a str),
    EventId(&'a str),
}

impl ParquetStorage {
    pub(crate) fn visit_retained_entity(
        &self,
        tenant_id: &str,
        entity_id: &str,
        budget: &mut ArchiveReadBudget,
        mut visit: impl FnMut(&Event, &ArchiveReadBudget) -> Result<()>,
    ) -> Result<()> {
        self.visit_retained_selection(
            tenant_id,
            Selection::Entity(entity_id),
            false,
            budget,
            |event, work, _| visit(event, work),
        )
        .map(|_| ())
    }

    pub(crate) fn visit_retained_prefix(
        &self,
        tenant_id: &str,
        prefix: &str,
        budget: &mut ArchiveReadBudget,
        mut visit: impl FnMut(&Event, &ArchiveReadBudget) -> Result<()>,
    ) -> Result<()> {
        self.visit_retained_selection(
            tenant_id,
            Selection::Prefix(prefix),
            false,
            budget,
            |event, work, _| visit(event, work),
        )
        .map(|_| ())
    }

    pub(crate) fn visit_event_history(
        &self,
        tenant_id: &str,
        selection: &crate::store::history::HistorySelection,
        budget: &mut ArchiveReadBudget,
        visit: impl FnMut(&Event, &ArchiveReadBudget, bool) -> Result<()>,
    ) -> Result<bool> {
        use crate::store::history::HistorySelection;
        match selection {
            HistorySelection::Timeline { entity_id, .. } => self.visit_retained_selection(
                tenant_id,
                Selection::Entity(entity_id),
                true,
                budget,
                visit,
            ),
            HistorySelection::Evidence(id) => self.visit_retained_selection(
                tenant_id,
                Selection::EventId(&id.to_string()),
                true,
                budget,
                visit,
            ),
        }
    }

    fn visit_retained_selection(
        &self,
        tenant_id: &str,
        selection: Selection<'_>,
        certify: bool,
        budget: &mut ArchiveReadBudget,
        mut visit: impl FnMut(&Event, &ArchiveReadBudget, bool) -> Result<()>,
    ) -> Result<bool> {
        let paths = self.list_complete_tenant_archive(tenant_id, budget)?;
        let certified = !certify || self.check_history_inventory(tenant_id, &paths, budget)?;
        for path in paths {
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
                .filter(|(_, group)| match selection {
                    Selection::Entity(id) => row_group_may_hold_entity(group, id.as_bytes()),
                    _ => true,
                })
                .map(|(index, _)| index)
                .collect();
            for batch in builder
                .with_row_groups(groups)
                .with_batch_size(256)
                .build()?
            {
                budget.check()?;
                let batch = batch?;
                let column = match selection {
                    Selection::Entity(_) => "entity_id",
                    Selection::Prefix(_) => "event_type",
                    Selection::EventId(_) => "event_id",
                };
                let ids = batch
                    .column_by_name(column)
                    .and_then(|column| column.as_any().downcast_ref::<arrow::array::StringArray>())
                    .ok_or_else(|| {
                        AllSourceError::StorageError(format!("Invalid {column} column"))
                    })?;
                if ids.null_count() != 0 {
                    return Err(AllSourceError::StorageError(format!(
                        "Null {column} in retained archive"
                    )));
                }
                let mask = BooleanArray::from_iter(ids.iter().map(|id| {
                    Some(id.is_some_and(|id| match selection {
                        Selection::Entity(exact) | Selection::EventId(exact) => id == exact,
                        Selection::Prefix(prefix) => id.starts_with(prefix),
                    }))
                }));
                let selected = filter_record_batch(&batch, &mask)?;
                let fractions = selected.column_by_name(RECORD_NANO_REMAINDER);
                // Charge retained payload before JSON decoding; unrelated rows never become Events.
                budget.decoded_payload(decoded_text_bytes(&selected))?;
                for (row, event) in self
                    .record_batch_to_events(&selected, tenant_id)?
                    .iter()
                    .enumerate()
                {
                    visit(
                        event,
                        budget,
                        fractions.is_some_and(|column| !column.is_null(row)),
                    )?;
                }
            }
        }
        budget.check()?;
        Ok(certified)
    }

    fn check_history_inventory(
        &self,
        tenant_id: &str,
        paths: &[PathBuf],
        budget: &mut ArchiveReadBudget,
    ) -> Result<bool> {
        use super::super::version_manifest::MANIFEST_FILE_NAME;
        let root = self
            .storage_dir
            .join(sanitize_tenant_id_for_path(tenant_id)?);
        let unavailable =
            || AllSourceError::StorageError("Retained history inventory unavailable".into());
        if !self.storage_dir.is_dir() {
            return Err(unavailable());
        }
        let manifest = match fs::File::open(root.join(MANIFEST_FILE_NAME)) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => return Err(unavailable()),
        };
        let size = manifest.metadata().map_err(|_| unavailable())?.len();
        budget.file_size(size)?;
        budget.compressed(size)?;
        let manifest: VersionManifest =
            serde_json::from_reader(manifest).map_err(|_| unavailable())?;
        let mut keys = std::collections::BTreeSet::new();
        for path in paths {
            budget.check()?;
            keys.insert(manifest_key(&root, path).ok_or_else(unavailable)?);
        }
        if !manifest.covered.is_subset(&keys) {
            return Err(unavailable());
        }
        Ok(manifest.covered == keys)
    }
}
