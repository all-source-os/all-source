use super::*;
use parquet::{
    data_type::Int64Type,
    file::{
        properties::WriterProperties, reader::FileReader, serialized_reader::SerializedFileReader,
        writer::SerializedFileWriter,
    },
    schema::parser::parse_message_type,
};
use std::sync::Arc;
use tempfile::TempDir;

fn fixture(root: &Path, tenant: &str, count: usize) -> std::path::PathBuf {
    let dir = root.join(tenant).join("2026-09");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("events-synthetic.parquet");
    let schema = Arc::new(parse_message_type("message synthetic { REQUIRED INT64 id; }").unwrap());
    let mut writer = SerializedFileWriter::new(
        File::create(&path).unwrap(),
        schema,
        Arc::new(WriterProperties::default()),
    )
    .unwrap();
    let mut group = writer.next_row_group().unwrap();
    let mut column = group.next_column().unwrap().unwrap();
    column
        .typed::<Int64Type>()
        .write_batch(&vec![1; count], None, None)
        .unwrap();
    column.close().unwrap();
    group.close().unwrap();
    writer.close().unwrap();
    path
}

#[test]
fn aggregate_rows_without_disclosing_tenant_names_or_mutating_files() {
    let dir = TempDir::new().unwrap();
    let first = fixture(dir.path(), "synthetic-private-a", 3);
    fixture(dir.path(), "synthetic-private-b", 5);
    let original = fs::read(&first).unwrap();
    let system = dir.path().join("system");
    fs::create_dir(&system).unwrap();
    fs::write(system.join("events-platform.parquet"), b"not decoded").unwrap();
    let report = audit(dir.path(), &Limits::default()).unwrap();
    assert_eq!(report.other.tenants, 2);
    assert_eq!(report.other.totals.rows, 8);
    assert_eq!(report.other.largest_tenant_rows, 5);
    assert!(report.other.totals.uncompressed_bytes > 0);
    assert_eq!(report.system.files, 1);
    let json = serde_json::to_value(report).unwrap();
    assert!(json["system"].get("rows").is_none());
    assert!(!json.to_string().contains("synthetic-private"));
    assert_eq!(fs::read(first).unwrap(), original);
}

#[test]
fn every_core_input_dimension_is_reported_per_tenant() {
    let dir = TempDir::new().unwrap();
    fixture(dir.path(), "synthetic", 3);
    let limits = Limits {
        tenant_entries: 1,
        tenant_files: 0,
        file_bytes: 0,
        compressed_bytes: 0,
        uncompressed_bytes: 0,
        rows: 2,
        ..Limits::default()
    };
    let report = audit(dir.path(), &limits).unwrap();
    let group = report.other;
    assert_eq!(group.tenants_over_entry_cap, 1);
    assert_eq!(group.tenants_over_file_cap, 1);
    assert_eq!(group.tenants_over_individual_file_cap, 1);
    assert_eq!(group.tenants_over_compressed_cap, 1);
    assert_eq!(group.tenants_over_uncompressed_cap, 1);
    assert_eq!(group.tenants_over_row_cap, 1);
}

#[test]
fn malformed_or_oversized_footer_refuses_without_cleanup() {
    let dir = TempDir::new().unwrap();
    let path = fixture(dir.path(), "synthetic", 1);
    let original = fs::read(&path).unwrap();
    let tiny = Limits {
        footer_bytes: 1,
        ..Limits::default()
    };
    assert_eq!(
        audit(dir.path(), &tiny).err(),
        Some("invalid or oversized parquet footer")
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    fs::write(&path, b"synthetic corrupt parquet file").unwrap();
    assert_eq!(
        audit(dir.path(), &Limits::default()).err(),
        Some("invalid or oversized parquet footer")
    );
    assert_eq!(fs::read(path).unwrap(), b"synthetic corrupt parquet file");
}

#[test]
fn data_page_corruption_is_not_misrepresented_as_payload_validation() {
    let dir = TempDir::new().unwrap();
    let path = fixture(dir.path(), "synthetic", 3);
    let reader = SerializedFileReader::new(File::open(&path).unwrap()).unwrap();
    let (offset, length) = reader.metadata().row_group(0).column(0).byte_range();
    drop(reader);
    let mut bytes = fs::read(&path).unwrap();
    bytes[offset as usize..(offset + length) as usize].fill(0);
    fs::write(&path, &bytes).unwrap();
    let report = audit(dir.path(), &Limits::default()).unwrap();
    assert_eq!(report.other.totals.rows, 3);
    assert!(!report.payloads_decoded);
    assert!(!report.data_pages_read);
    assert!(!report.atomic_snapshot);
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn traversal_time_and_entry_caps_refuse_instead_of_returning_partial_success() {
    let dir = TempDir::new().unwrap();
    fixture(dir.path(), "synthetic", 3);
    for (limits, error) in [
        (
            Limits {
                timeout: Duration::ZERO,
                ..Limits::default()
            },
            "scan deadline",
        ),
        (
            Limits {
                scan_entries: 1,
                ..Limits::default()
            },
            "scan entry cap",
        ),
    ] {
        assert_eq!(audit(dir.path(), &limits).err(), Some(error));
    }
    let deep = dir.path().join("other").join("a/b/c/d/e/f/g/h");
    fs::create_dir_all(deep).unwrap();
    assert_eq!(
        audit(dir.path(), &Limits::default()).err(),
        Some("directory depth cap")
    );
}

#[cfg(unix)]
#[test]
fn symlinks_and_unattributed_flat_files_are_refused() {
    let dir = TempDir::new().unwrap();
    std::os::unix::fs::symlink(dir.path(), dir.path().join("cycle")).unwrap();
    assert_eq!(
        audit(dir.path(), &Limits::default()).err(),
        Some("symlink refused")
    );
    fs::remove_file(dir.path().join("cycle")).unwrap();
    fs::write(dir.path().join("events-flat.parquet"), b"unattributed").unwrap();
    assert_eq!(
        audit(dir.path(), &Limits::default()).err(),
        Some("legacy flat archive needs separate tenant attribution")
    );
}
