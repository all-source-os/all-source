use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
            "customer-reader-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }

    fn file(&self, bytes: &[u8]) -> PathBuf {
        let path = self.0.join("connection.json");
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_allsource-customer-connection"))
        .arg(path)
        .env("UID", "0")
        .env("USER", "root")
        .output()
        .unwrap()
}

fn denied(path: &Path) {
    let result = run(path);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
}

#[test]
fn reads_exact_bytes_and_reloads_with_real_identity_despite_spoofed_environment() {
    let fixture = Fixture::new();
    let path = fixture.file(b"synthetic-first");
    assert_eq!(run(&path).stdout, b"synthetic-first");
    fs::write(&path, b"synthetic-second").unwrap();
    let result = run(&path);
    assert!(result.status.success());
    assert_eq!(result.stdout, b"synthetic-second");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    assert!(run(&path).status.success());
}

#[test]
fn denies_shared_executable_and_special_file_modes_without_printing_secrets() {
    let fixture = Fixture::new();
    let path = fixture.file(b"synthetic-private-marker");
    for mode in [0o644, 0o640, 0o660, 0o700, 0o4600, 0o000] {
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        denied(&path);
    }
}

#[test]
fn denies_non_private_parent_and_unsafe_ancestor() {
    let fixture = Fixture::new();
    let path = fixture.file(b"synthetic-private-marker");
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o755)).unwrap();
    denied(&path);
    let child = fixture.0.join("private");
    fs::create_dir(&child).unwrap();
    fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
    let nested = child.join("connection.json");
    fs::rename(&path, &nested).unwrap();
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o777)).unwrap();
    denied(&nested);
}

#[test]
fn denies_file_and_directory_symlinks_hardlinks_and_parent_traversal() {
    let fixture = Fixture::new();
    let path = fixture.file(b"synthetic-private-marker");
    let link = fixture.0.join("link");
    symlink(&path, &link).unwrap();
    denied(&link);
    fs::remove_file(&link).unwrap();
    fs::hard_link(&path, &link).unwrap();
    denied(&path);
    denied(&link);
    fs::remove_file(&link).unwrap();
    symlink(&fixture.0, &link).unwrap();
    denied(&link.join("connection.json"));
    denied(
        &fixture
            .0
            .join("../")
            .join(fixture.0.file_name().unwrap())
            .join("connection.json"),
    );
    denied(Path::new("connection.json"));
}

#[test]
fn bounds_bytes_and_denies_special_files_without_blocking() {
    let fixture = Fixture::new();
    let path = fixture.file(&vec![b'x'; 8192]);
    assert_eq!(run(&path).stdout.len(), 8192);
    fs::write(&path, vec![b'x'; 8193]).unwrap();
    denied(&path);
    fs::write(&path, b"").unwrap();
    denied(&path);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    denied(&path);
    fs::remove_dir(&path).unwrap();
    let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: path is a valid NUL-terminated name inside the owned test directory.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let start = std::time::Instant::now();
    denied(&path);
    assert!(start.elapsed().as_secs() < 2);
}

#[cfg(target_os = "macos")]
#[test]
fn denies_acl_read_grants_despite_private_posix_modes() {
    let fixture = Fixture::new();
    let path = fixture.file(b"synthetic-private-marker");
    assert!(
        Command::new("/bin/chmod")
            .args(["+a", "everyone allow read"])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    denied(&path);
    assert!(
        Command::new("/bin/chmod")
            .arg("-N")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    assert!(run(&path).status.success());
    assert!(
        Command::new("/bin/chmod")
            .args(["+a", "everyone allow list,search"])
            .arg(&fixture.0)
            .status()
            .unwrap()
            .success()
    );
    denied(&path);
}

#[cfg(target_os = "linux")]
#[test]
fn denies_extended_posix_acl_even_with_zero_effective_mask() {
    use std::os::fd::AsRawFd;
    let fixture = Fixture::new();
    let path = fixture.file(b"synthetic-private-marker");
    let file = fs::File::open(&path).unwrap();
    let mut acl = 2_u32.to_le_bytes().to_vec();
    for (tag, perm, id) in [
        (1_u16, 6_u16, u32::MAX),
        (2, 4, 65534),
        (4, 0, u32::MAX),
        (16, 0, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        acl.extend_from_slice(&tag.to_le_bytes());
        acl.extend_from_slice(&perm.to_le_bytes());
        acl.extend_from_slice(&id.to_le_bytes());
    }
    // SAFETY: all pointers refer to live bounded buffers; descriptor belongs to
    // this test. This synthetic ACL grants no effective access to other users.
    let status = unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            c"system.posix_acl_access".as_ptr(),
            acl.as_ptr().cast(),
            acl.len(),
            0,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    denied(&path);
}
