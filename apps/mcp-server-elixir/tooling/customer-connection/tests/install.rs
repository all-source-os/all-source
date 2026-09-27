use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
};

#[test]
fn installs_private_bytes_without_echo_and_never_overwrites_or_follows_links() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("customer-install-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let directory = root.join("new-private");
    let path = directory.join("connection.json");
    let bytes = br#"{"version":1,"url":"https://api.example.test","token":"synthetic-private-marker","binding":{}}"#;
    let install = |path: &std::path::Path, bytes: &[u8]| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_allsource-customer-connection"))
            .arg("--install")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(bytes).unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
        result.status.success()
    };
    assert!(install(&path, bytes));
    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o7777,
        0o700
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
        0o600
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert!(!install(&path, b"replacement"));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let result = Command::new(env!("CARGO_BIN_EXE_allsource-customer-connection"))
        .arg(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, bytes);

    let linked = root.join("link");
    std::os::unix::fs::symlink(&directory, &linked).unwrap();
    assert!(!install(&linked.join("other.json"), bytes));
    assert!(!directory.join("other.json").exists());
    assert!(!install(&directory.join("oversized"), &vec![b'x'; 8193]));
    assert!(!directory.join("oversized").exists());
    assert!(!install(&directory.join("empty"), b""));
    assert!(!install(
        &directory.join("not-json"),
        b"pbpaste | installer"
    ));
    assert!(!directory.join("not-json").exists());
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!install(&directory.join("shared"), bytes));
    fs::remove_dir_all(root).unwrap();
}
