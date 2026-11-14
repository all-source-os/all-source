//! libtest summary parsing for the core-tests evaluator.

use core_heap_replay::steps::{TestCounts, parse_test_result};

#[test]
fn single_summary_line_is_read() {
    let out = "running 1774 tests\ntest x ... ok\n\ntest result: ok. 1772 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 26.84s\n";
    assert_eq!(
        parse_test_result(out),
        Some(TestCounts {
            passed: 1772,
            failed: 0,
            ignored: 2
        })
    );
}

#[test]
fn one_summary_per_test_binary_is_summed() {
    let out = "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n\
               test result: FAILED. 4 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out\n";
    assert_eq!(
        parse_test_result(out),
        Some(TestCounts {
            passed: 7,
            failed: 1,
            ignored: 1
        })
    );
}

#[test]
fn output_without_a_summary_is_none() {
    assert_eq!(parse_test_result("error[E0432]: unresolved import\n"), None);
}
