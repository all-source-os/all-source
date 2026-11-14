//! The objective is the replay peak floored to whole MiB, so the ~1.4 KB
//! run-to-run spread of the raw peak cannot change it.

use core_heap_replay::replay::{PEAK_QUANTUM_BYTES, objective_peak};

#[test]
fn objective_peak_floors_to_the_quantum() {
    assert_eq!(PEAK_QUANTUM_BYTES, 1 << 20);
    assert_eq!(objective_peak(0), 0);
    assert_eq!(objective_peak(PEAK_QUANTUM_BYTES - 1), 0);
    assert_eq!(objective_peak(PEAK_QUANTUM_BYTES), PEAK_QUANTUM_BYTES);

    let measured = [1_301_854_276, 1_301_855_656, 1_301_853_994, 1_301_854_851];
    let floored: Vec<u64> = measured.iter().map(|&p| objective_peak(p)).collect();
    assert!(floored.iter().all(|&p| p == 1_241 * PEAK_QUANTUM_BYTES));
}
