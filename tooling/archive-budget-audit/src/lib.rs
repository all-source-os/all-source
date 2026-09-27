use parquet::file::metadata::ParquetMetaDataReader;
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, &'static str>;

/// Matches Core's current input policy. Scan bounds belong only to this tool.
pub struct Limits {
    pub tenant_entries: u64,
    pub tenant_files: u64,
    pub file_bytes: u64,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    pub rows: u64,
    pub scan_entries: u64,
    pub footer_bytes: u64,
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            tenant_entries: 100_000,
            tenant_files: 50_000,
            file_bytes: 32 * 1024 * 1024,
            compressed_bytes: 256 * 1024 * 1024,
            uncompressed_bytes: 256 * 1024 * 1024,
            rows: 250_000,
            scan_entries: 300_000,
            footer_bytes: 1024 * 1024,
            timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Default, Serialize)]
pub struct Totals {
    pub entries: u64,
    pub files: u64,
    pub compressed_bytes: u64,
    pub max_file_bytes: u64,
    pub rows: u64,
    pub uncompressed_bytes: u64,
}

#[derive(Default, Serialize)]
pub struct Group {
    pub tenants: u64,
    pub totals: Totals,
    pub largest_tenant_files: u64,
    pub largest_tenant_entries: u64,
    pub largest_tenant_compressed_bytes: u64,
    pub largest_tenant_rows: u64,
    pub largest_tenant_uncompressed_bytes: u64,
    pub largest_row_archive: Option<Totals>,
    pub tenants_over_file_cap: u64,
    pub tenants_over_entry_cap: u64,
    pub tenants_over_individual_file_cap: u64,
    pub tenants_over_compressed_cap: u64,
    pub tenants_over_row_cap: u64,
    pub tenants_over_uncompressed_cap: u64,
}

/// Only filesystem facts are observed for the platform system tenant.
#[derive(Serialize)]
pub struct SystemSummary {
    pub tenants: u64,
    pub entries: u64,
    pub files: u64,
    pub compressed_bytes: u64,
    pub max_file_bytes: u64,
    pub tenants_over_entry_cap: u64,
    pub tenants_over_file_cap: u64,
    pub tenants_over_compressed_cap: u64,
    pub tenants_over_individual_file_cap: u64,
}

impl From<Group> for SystemSummary {
    fn from(group: Group) -> Self {
        Self {
            tenants: group.tenants,
            entries: group.totals.entries,
            files: group.totals.files,
            compressed_bytes: group.totals.compressed_bytes,
            max_file_bytes: group.totals.max_file_bytes,
            tenants_over_entry_cap: group.tenants_over_entry_cap,
            tenants_over_file_cap: group.tenants_over_file_cap,
            tenants_over_compressed_cap: group.tenants_over_compressed_cap,
            tenants_over_individual_file_cap: group.tenants_over_individual_file_cap,
        }
    }
}

#[derive(Serialize)]
pub struct Report {
    pub protocol: &'static str,
    pub payloads_decoded: bool,
    pub data_pages_read: bool,
    pub atomic_snapshot: bool,
    pub system_rows_scanned: bool,
    pub system: SystemSummary,
    pub other: Group,
    pub entries: u64,
    pub elapsed_ms: u128,
}

struct Scan<'a> {
    limits: &'a Limits,
    started: Instant,
    entries: u64,
}

impl Scan<'_> {
    fn check(&self) -> Result<()> {
        if self.started.elapsed() >= self.limits.timeout {
            return Err("scan deadline");
        }
        Ok(())
    }

    fn entry(&mut self) -> Result<()> {
        self.check()?;
        add(&mut self.entries, 1)?;
        if self.entries > self.limits.scan_entries {
            return Err("scan entry cap");
        }
        Ok(())
    }

    fn directory(
        &mut self,
        path: &Path,
        depth: usize,
        system: bool,
        total: &mut Totals,
    ) -> Result<()> {
        self.check()?;
        if depth > 8 {
            return Err("directory depth cap");
        }
        for entry in fs::read_dir(path).map_err(|_| "unreadable directory")? {
            self.entry()?;
            add(&mut total.entries, 1)?;
            let entry = entry.map_err(|_| "unreadable directory entry")?;
            let kind = entry.file_type().map_err(|_| "unreadable entry type")?;
            if kind.is_symlink() {
                return Err("symlink refused");
            }
            if kind.is_dir() {
                self.directory(&entry.path(), depth + 1, system, total)?;
            } else if kind.is_file() && entry.path().extension().is_some_and(|ext| ext == "parquet")
            {
                let file = File::open(entry.path()).map_err(|_| "unreadable parquet file")?;
                let length = file.metadata().map_err(|_| "unreadable file size")?.len();
                add(&mut total.files, 1)?;
                add(&mut total.compressed_bytes, length)?;
                total.max_file_bytes = total.max_file_bytes.max(length);
                // The known oversized platform archive is counted, not decoded.
                // Avoid 219k unnecessary footer reads on a live shared machine.
                if !system {
                    let (rows, bytes) = footer(file, self.limits.footer_bytes)?;
                    add(&mut total.rows, rows)?;
                    add(&mut total.uncompressed_bytes, bytes)?;
                }
            }
        }
        self.check()
    }
}

/// Reads only file/footer metadata. Never constructs a Core store or cleans files.
pub fn audit(root: &Path, limits: &Limits) -> Result<Report> {
    let kind = fs::symlink_metadata(root)
        .map_err(|_| "unreadable root")?
        .file_type();
    if !kind.is_dir() || kind.is_symlink() {
        return Err("root must be a directory without symlinks");
    }
    let mut scan = Scan {
        limits,
        started: Instant::now(),
        entries: 0,
    };
    let mut system = Group::default();
    let mut other = Group::default();
    for entry in fs::read_dir(root).map_err(|_| "unreadable root directory")? {
        scan.entry()?;
        let entry = entry.map_err(|_| "unreadable tenant directory")?;
        let kind = entry.file_type().map_err(|_| "unreadable tenant type")?;
        if kind.is_symlink() {
            return Err("symlink refused");
        }
        if !kind.is_dir() {
            if entry.path().extension().is_some_and(|ext| ext == "parquet") {
                return Err("legacy flat archive needs separate tenant attribution");
            }
            continue;
        }
        let is_system = entry.file_name() == "system";
        let mut total = Totals::default();
        scan.directory(&entry.path(), 1, is_system, &mut total)?;
        if total.files > 0 {
            aggregate(
                if is_system { &mut system } else { &mut other },
                total,
                limits,
            )?;
        }
    }
    scan.check()?;
    Ok(Report {
        protocol: "archive-budget-metadata-v1",
        payloads_decoded: false,
        data_pages_read: false,
        atomic_snapshot: false,
        system_rows_scanned: false,
        system: system.into(),
        other,
        entries: scan.entries,
        elapsed_ms: scan.started.elapsed().as_millis(),
    })
}

fn footer(mut file: File, cap: u64) -> Result<(u64, u64)> {
    let before = file.metadata().map_err(|_| "unreadable file metadata")?;
    if before.len() < 12 {
        return Err("truncated parquet footer");
    }
    file.seek(SeekFrom::End(-8))
        .map_err(|_| "unseekable parquet footer")?;
    let mut tail = [0_u8; 8];
    file.read_exact(&mut tail)
        .map_err(|_| "unreadable parquet footer")?;
    let size = u64::from(u32::from_le_bytes(
        tail[..4].try_into().expect("four bytes"),
    ));
    if &tail[4..] != b"PAR1" || size > before.len() - 12 || size > cap {
        return Err("invalid or oversized parquet footer");
    }
    // Decode exactly the bounded bytes inspected above. Giving the parser a
    // live File would let a concurrently replaced footer request a larger read.
    file.seek(SeekFrom::Start(before.len() - 8 - size))
        .map_err(|_| "unseekable parquet metadata")?;
    let mut bytes = vec![0; usize::try_from(size).map_err(|_| "footer size overflow")?];
    file.read_exact(&mut bytes)
        .map_err(|_| "unreadable parquet metadata")?;
    let metadata =
        ParquetMetaDataReader::decode_metadata(&bytes).map_err(|_| "invalid parquet metadata")?;
    let mut rows = 0;
    let mut bytes = 0;
    for group in metadata.row_groups() {
        add(
            &mut rows,
            u64::try_from(group.num_rows()).map_err(|_| "negative row count")?,
        )?;
        add(
            &mut bytes,
            u64::try_from(group.total_byte_size()).map_err(|_| "negative decoded size")?,
        )?;
    }
    let after = file
        .metadata()
        .map_err(|_| "unreadable final file metadata")?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("archive changed during footer read");
    }
    Ok((rows, bytes))
}

fn aggregate(group: &mut Group, total: Totals, limits: &Limits) -> Result<()> {
    if group
        .largest_row_archive
        .as_ref()
        .is_none_or(|current| total.rows > current.rows)
    {
        group.largest_row_archive = Some(total.clone());
    }
    add(&mut group.tenants, 1)?;
    add(&mut group.totals.entries, total.entries)?;
    add(&mut group.totals.files, total.files)?;
    add(&mut group.totals.compressed_bytes, total.compressed_bytes)?;
    add(&mut group.totals.rows, total.rows)?;
    add(
        &mut group.totals.uncompressed_bytes,
        total.uncompressed_bytes,
    )?;
    group.totals.max_file_bytes = group.totals.max_file_bytes.max(total.max_file_bytes);
    group.largest_tenant_files = group.largest_tenant_files.max(total.files);
    group.largest_tenant_entries = group.largest_tenant_entries.max(total.entries);
    group.largest_tenant_compressed_bytes = group
        .largest_tenant_compressed_bytes
        .max(total.compressed_bytes);
    group.largest_tenant_rows = group.largest_tenant_rows.max(total.rows);
    group.largest_tenant_uncompressed_bytes = group
        .largest_tenant_uncompressed_bytes
        .max(total.uncompressed_bytes);
    group.tenants_over_file_cap += u64::from(total.files > limits.tenant_files);
    group.tenants_over_entry_cap += u64::from(total.entries > limits.tenant_entries);
    group.tenants_over_individual_file_cap += u64::from(total.max_file_bytes > limits.file_bytes);
    group.tenants_over_compressed_cap +=
        u64::from(total.compressed_bytes > limits.compressed_bytes);
    group.tenants_over_row_cap += u64::from(total.rows > limits.rows);
    group.tenants_over_uncompressed_cap +=
        u64::from(total.uncompressed_bytes > limits.uncompressed_bytes);
    Ok(())
}

fn add(total: &mut u64, value: u64) -> Result<()> {
    *total = total.checked_add(value).ok_or("metadata count overflow")?;
    Ok(())
}

#[cfg(test)]
mod tests;
