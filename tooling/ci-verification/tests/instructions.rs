use allsource_ci_verification::{baseline, compare};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const BASE: &str = "\
iai::group::kept
  Instructions: 10,000|N/A (*********)
iai::group::other
  Instructions: 500|N/A (*********)
";

#[test]
fn rejects_observed_zero_instruction_success() {
    let output = include_str!("fixtures/zero-instructions.txt");
    assert!(baseline(output).is_err());
    assert!(compare(output, BASE, 3).is_err());
}

#[test]
fn baseline_allows_first_measurement_but_comparison_requires_prior_counts() {
    let output = "Instructions: 1,234|N/A (*********)";
    assert_eq!(baseline(output).unwrap().measurements, 1);
    assert!(compare(output, BASE, 3).is_err());
}

#[test]
fn checks_exact_threshold_without_rounding() {
    let report = compare(
        "iai::group::kept\n  Instructions: 10,300|10,000 (+3.000%)",
        BASE,
        3,
    )
    .unwrap();
    assert_eq!(report.worst_basis_points, 300);
    assert!(
        compare(
            "iai::group::kept\n  Instructions: 10,301|10,000 (+3.010%)",
            BASE,
            3
        )
        .is_err()
    );
}

#[test]
fn accepts_equal_or_improved_ansi_measurements() {
    let output = "\u{1b}[32miai::group::kept\u{1b}[0m\n  \u{1b}[32mInstructions: 900|1,000 (-10.0%)\u{1b}[0m\niai::group::other\n  Instructions: 500|500 (No change)";
    let report = compare(output, BASE, 3).unwrap();
    assert_eq!(report.measurements, 2);
    assert_eq!(report.worst_basis_points, 0);
}

#[test]
fn rejects_missing_or_zero_baselines_and_later_bad_cases() {
    for tail in [
        "Instructions: 10",
        "Instructions: 10|0",
        "Instructions: 104|100",
    ] {
        assert!(compare(&format!("Instructions: 100|100\n{tail}"), BASE, 3).is_err());
    }
}

#[test]
fn rejects_empty_or_malformed_measurements() {
    for output in ["", "benchmarks finished", "Instructions: invalid|100"] {
        assert!(compare(output, BASE, 3).is_err());
    }
}

#[test]
fn handles_full_width_counters_without_overflow() {
    let output = format!("Instructions: {}|{}", u64::MAX, u64::MAX);
    assert_eq!(compare(&output, BASE, 3).unwrap().measurements, 1);
    assert!(compare(&format!("Instructions: {}|1", u64::MAX), BASE, 3).is_err());
}

#[test]
fn a_case_the_base_never_measured_is_reported_as_added() {
    let output = "iai::group::kept\n  Instructions: 10,000|10,000 (No change)\n\
                  iai::group::brand_new\n  Instructions: 777|N/A (*********)";
    let report = compare(output, BASE, 3).unwrap();
    assert_eq!(report.measurements, 1);
    assert_eq!(report.added, ["iai::group::brand_new"]);
}

#[test]
fn a_case_the_base_measured_must_stay_paired() {
    let output = "iai::group::kept\n  Instructions: 10,000|10,000 (No change)\n\
                  iai::group::other\n  Instructions: 500|N/A (*********)";
    let error = compare(output, BASE, 3).unwrap_err();
    assert!(error.contains("iai::group::other"), "{error}");
}

#[test]
fn renaming_every_case_leaves_nothing_paired() {
    let output = "iai::group::renamed\n  Instructions: 10,000|N/A (*********)";
    assert!(compare(output, BASE, 3).is_err());
    assert!(compare(output, "", 3).is_err());
}

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn cli_rejects_observed_zero_instructions() {
    let mut child = OwnedChild(
        Command::new(env!("CARGO_BIN_EXE_allsource-ci-verification"))
            .arg("baseline")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/zero-instructions.txt"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert_eq!(status.code(), Some(1));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "instruction gate exceeded deadline"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
