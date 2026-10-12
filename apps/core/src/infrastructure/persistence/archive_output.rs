use crate::error::{AllSourceError, Result};
use arrow::record_batch::RecordBatch;
use parquet::{
    arrow::ArrowWriter,
    basic::Compression,
    file::{metadata::ParquetMetaData, properties::WriterProperties},
};
use std::{
    fs::{self, File},
    path::{Component, Path, PathBuf},
};

pub(crate) struct PublishedPart {
    pub(crate) path: PathBuf,
    pub(crate) metadata: ParquetMetaData,
}

struct StagedPart {
    temporary: PathBuf,
    published: PublishedPart,
}

pub(crate) struct ArchiveOutput {
    directory: PathBuf,
    stem: String,
    compression: Compression,
    max_bytes: u64,
    split: bool,
    failed: bool,
    parts: Vec<StagedPart>,
    candidates: Vec<PathBuf>,
    pending: Option<PathBuf>,
    renamed: Vec<PathBuf>,
}

impl ArchiveOutput {
    pub(crate) fn new(
        directory: &Path,
        stem: &str,
        compression: Compression,
        max_bytes: u64,
        split: bool,
    ) -> Result<Self> {
        let mut components = Path::new(stem).components();
        if !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
            || max_bytes == 0
        {
            return Err(AllSourceError::StorageError(
                "Invalid archive output name or byte limit".into(),
            ));
        }
        Ok(Self {
            directory: directory.to_path_buf(),
            stem: if split {
                format!("{stem}.{}", uuid::Uuid::new_v4().as_simple())
            } else {
                stem.to_owned()
            },
            compression,
            max_bytes,
            split,
            failed: false,
            parts: Vec::new(),
            candidates: Vec::new(),
            pending: None,
            renamed: Vec::new(),
        })
    }

    pub(crate) fn stage(&mut self, batch: &RecordBatch) -> Result<()> {
        if self.failed {
            return Err(AllSourceError::StorageError(
                "Archive output has failed".into(),
            ));
        }
        let result = self.stage_batch(batch);
        self.failed = result.is_err();
        result
    }

    fn stage_batch(&mut self, batch: &RecordBatch) -> Result<()> {
        if batch.num_rows() == 0 {
            return Err(AllSourceError::StorageError(
                "Cannot publish an empty archive batch".into(),
            ));
        }
        let name = if self.split {
            format!("{}.part-{}", self.stem, self.parts.len())
        } else {
            self.stem.clone()
        };
        let final_path = self.directory.join(format!("{name}.parquet"));
        let temporary = self.directory.join(format!("{name}.parquet.tmp"));
        self.candidates.push(temporary.clone());
        let file = File::create(&temporary).map_err(io_error)?;
        let properties = WriterProperties::builder()
            .set_compression(self.compression)
            .set_statistics_truncate_length(None)
            .build();
        let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(properties))?;
        writer.write(batch)?;
        let metadata = writer.close()?;
        let file = File::open(&temporary).map_err(io_error)?;
        let bytes = file.metadata().map_err(io_error)?.len();
        if bytes > self.max_bytes {
            drop(file);
            fs::remove_file(&temporary).map_err(io_error)?;
            if !self.split || batch.num_rows() == 1 {
                return Err(AllSourceError::StorageError(format!(
                    "Archive output exceeds {} byte file limit",
                    self.max_bytes
                )));
            }
            let middle = batch.num_rows() / 2;
            self.stage_batch(&batch.slice(0, middle))?;
            return self.stage_batch(&batch.slice(middle, batch.num_rows() - middle));
        }
        file.sync_all().map_err(io_error)?;
        self.parts.push(StagedPart {
            temporary,
            published: PublishedPart {
                path: final_path,
                metadata,
            },
        });
        Ok(())
    }

    pub(crate) fn publish(mut self) -> Result<Vec<PublishedPart>> {
        if self.failed {
            return Err(AllSourceError::StorageError(
                "Archive output has failed".into(),
            ));
        }
        if self.parts.is_empty() {
            return Err(AllSourceError::StorageError(
                "Archive output has no staged parts".into(),
            ));
        }
        if self.split {
            let pending = self
                .directory
                .join(format!(".archive-output.{}.pending", self.stem));
            let temporary = pending.with_extension("pending.tmp");
            let names = self
                .parts
                .iter()
                .map(|part| {
                    part.published
                        .path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<Vec<_>>();
            self.candidates.push(temporary.clone());
            let mut file = File::options()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(io_error)?;
            use std::io::Write;
            file.write_all(&serde_json::to_vec(&names)?)
                .map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            fs::rename(temporary, &pending).map_err(io_error)?;
            self.pending = Some(pending);
            sync_directory(&self.directory)?;
        }
        let result = self.publish_parts();
        if let Err(error) = result {
            for path in &self.renamed {
                if let Err(cleanup) = fs::remove_file(path) {
                    tracing::error!(file = %path.display(), "failed to withdraw unpublished archive part: {cleanup}");
                }
            }
            let _ = sync_directory(&self.directory);
            return Err(error);
        }
        self.renamed.clear();
        Ok(std::mem::take(&mut self.parts)
            .into_iter()
            .map(|part| part.published)
            .collect())
    }

    fn publish_parts(&mut self) -> Result<()> {
        // Readers refuse pending generations until every part is durable.
        for part in &self.parts {
            fs::rename(&part.temporary, &part.published.path).map_err(io_error)?;
            self.renamed.push(part.published.path.clone());
        }
        sync_directory(&self.directory)?;
        if let Some(pending) = &self.pending {
            fs::remove_file(pending).map_err(io_error)?;
            sync_directory(&self.directory)?;
        }
        Ok(())
    }

    pub(crate) fn staged_files(&self) -> Vec<(&Path, &Path)> {
        self.parts
            .iter()
            .map(|part| (part.temporary.as_path(), part.published.path.as_path()))
            .collect()
    }
}

pub(crate) fn pending_marker(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with(".archive-output.") && name.ends_with(".pending"))
}

pub(crate) fn pending_part(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let Some((stem, part)) = name.rsplit_once(".part-") else {
        return false;
    };
    let Some(index) = part.strip_suffix(".parquet") else {
        return false;
    };
    if index.parse::<usize>().is_err() {
        return false;
    }
    path.parent().is_some_and(|directory| {
        directory
            .join(format!(".archive-output.{stem}.pending"))
            .exists()
    })
}

pub(crate) fn clear_pending_output(directory: &Path, outputs: &[String]) -> Result<()> {
    let Some((stem, _)) = outputs.first().and_then(|name| name.rsplit_once(".part-")) else {
        return Err(AllSourceError::StorageError(
            "Invalid archive generation".into(),
        ));
    };
    let marker = directory.join(format!(".archive-output.{stem}.pending"));
    let metadata = match fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error(error)),
    };
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(AllSourceError::StorageError(
            "Invalid pending archive marker".into(),
        ));
    }
    let names: Vec<String> = serde_json::from_slice(&fs::read(&marker).map_err(io_error)?)?;
    if names != outputs
        || outputs.iter().any(|name| {
            name.strip_prefix(&format!("{stem}.part-"))
                .and_then(|rest| rest.strip_suffix(".parquet"))
                .is_none_or(|index| index.parse::<usize>().is_err())
        })
    {
        return Err(AllSourceError::StorageError(
            "Pending archive generation differs".into(),
        ));
    }
    fs::remove_file(marker).map_err(io_error)?;
    sync_directory(directory)
}

pub(crate) fn recover_pending_outputs(directory: &Path) -> Result<usize> {
    let mut recovered = 0;
    for entry in fs::read_dir(directory).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        if !pending_marker(&path) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if !metadata.is_file() || metadata.len() > 1024 * 1024 {
            return Err(AllSourceError::StorageError(
                "Invalid pending archive marker".into(),
            ));
        }
        let name = path.file_name().unwrap().to_string_lossy();
        let stem = name
            .strip_prefix(".archive-output.")
            .unwrap()
            .strip_suffix(".pending")
            .unwrap();
        let names: Vec<String> = serde_json::from_slice(&fs::read(&path).map_err(io_error)?)?;
        for name in names {
            let prefix = format!("{stem}.part-");
            let valid = name
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(".parquet"))
                .is_some_and(|index| index.parse::<usize>().is_ok());
            if !valid {
                return Err(AllSourceError::StorageError(
                    "Invalid pending archive part".into(),
                ));
            }
            let part = directory.join(name);
            match fs::symlink_metadata(&part) {
                Ok(metadata) if metadata.is_file() => {
                    fs::remove_file(&part).map_err(io_error)?;
                    recovered += 1;
                }
                Ok(_) => {
                    return Err(AllSourceError::StorageError(
                        "Pending archive part is not a regular file".into(),
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error(error)),
            }
        }
        sync_directory(directory)?;
        fs::remove_file(&path).map_err(io_error)?;
        sync_directory(directory)?;
    }
    Ok(recovered)
}

fn sync_directory(directory: &Path) -> Result<()> {
    File::open(directory)
        .map_err(io_error)?
        .sync_all()
        .map_err(io_error)
}

fn io_error(error: impl std::error::Error) -> AllSourceError {
    AllSourceError::StorageError(format!("Archive output I/O failed: {error}"))
}

impl Drop for ArchiveOutput {
    fn drop(&mut self) {
        for path in &self.candidates {
            let _ = fs::remove_file(path);
        }
        if let Some(pending) = &self.pending
            && self.renamed.iter().all(|path| !path.exists())
        {
            let _ = fs::remove_file(pending);
            let _ = sync_directory(&self.directory);
        }
    }
}

#[cfg(test)]
#[path = "archive_output_tests.rs"]
mod tests;
