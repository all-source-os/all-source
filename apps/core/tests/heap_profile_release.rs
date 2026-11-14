//! Release feature sets must not link any jemalloc crate. `--target all` makes
//! `cargo tree` include the linux-only heap-profile dependencies even when the
//! test host is darwin.
use std::process::Command;

const RELEASE_SETS: [&str; 2] = ["community,analytics", "enterprise,analytics"];
const JEMALLOC_CRATES: [&str; 4] = [
    "tikv-jemallocator",
    "tikv-jemalloc-sys",
    "tikv-jemalloc-ctl",
    "jemalloc_pprof",
];

fn package_names(features: &str) -> Vec<String> {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "-p",
            "allsource-core",
            "-e",
            "normal",
            "--target",
            "all",
            "--prefix",
            "none",
            "--locked",
            "--features",
            features,
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree spawns");
    assert!(
        output.status.success(),
        "cargo tree --features {features} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("cargo tree output is utf-8")
        .lines()
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}

#[test]
fn release_build_has_no_heap_profile() {
    for features in RELEASE_SETS {
        let names = package_names(features);
        for crate_name in JEMALLOC_CRATES {
            assert!(
                !names.iter().any(|name| name == crate_name),
                "--features {features} links {crate_name}"
            );
        }
    }
}

#[test]
fn heap_profile_set_shows_jemalloc() {
    let names = package_names("heap-profile");
    assert!(
        names.iter().any(|name| name == "tikv-jemallocator"),
        "positive control failed: heap-profile tree has no tikv-jemallocator"
    );
}
