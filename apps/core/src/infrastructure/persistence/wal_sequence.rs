//! An erased WAL tail must not make a recovered writer reuse sequence numbers.

use crate::error::{AllSourceError, Result};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

const NAME: &str = ".erasure-sequence-floor";

fn error(error: impl std::fmt::Display) -> AllSourceError {
    AllSourceError::StorageError(format!("WAL erasure sequence floor: {error}"))
}

pub(super) fn read_floor(directory: &Path) -> Result<u64> {
    let path = directory.join(NAME);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(error(e)),
    };
    if !metadata.is_file() || metadata.len() > 20 {
        return Err(error("invalid watermark file"));
    }
    fs::read_to_string(path)
        .map_err(error)?
        .parse()
        .map_err(error)
}

/// Caller holds the directory maintenance lock and has stopped the WAL writer.
pub(super) fn preserve_floor(directory: &Path, sequence: u64) -> Result<()> {
    let floor = read_floor(directory)?;
    if sequence <= floor {
        return Ok(());
    }
    let temporary = directory.join(format!(".sequence-{}.pending", uuid::Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(error)?;
    let result = (|| {
        write!(file, "{sequence}").map_err(error)?;
        file.sync_all().map_err(error)?;
        fs::rename(&temporary, directory.join(NAME)).map_err(error)?;
        File::open(directory)
            .and_then(|dir| dir.sync_all())
            .map_err(error)
    })();
    drop(file);
    match fs::remove_file(temporary) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(error(e)),
    }
    result
}
