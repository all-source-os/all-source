use allsource_core::infrastructure::memory::ledger::{MapCharge, Owner};
use dashmap::DashMap;
use std::{sync::Arc, thread};

const WORKERS: u64 = 8;
const OPS_PER_WORKER: u64 = 20_000;
const KEYS: u64 = 16;

#[test]
fn map_charge_owned_bytes_equal_entries_under_contention() {
    let map: Arc<DashMap<u64, u64>> = Arc::new(DashMap::new());
    let charge = Arc::new(MapCharge::new(Owner::Misc, 16));

    let workers: Vec<_> = (0..WORKERS)
        .map(|worker| {
            let map = Arc::clone(&map);
            let charge = Arc::clone(&charge);
            thread::spawn(move || {
                for op in 0..OPS_PER_WORKER {
                    let key = (op * 7 + worker) % KEYS;
                    if op % 3 == 0 {
                        charge.remove_entry(&map, &key, |_, bytes| *bytes);
                    } else {
                        charge.upsert(
                            &map,
                            key,
                            || 10 + key,
                            |_, bytes| *bytes,
                            |bytes| *bytes += 1,
                        );
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("worker panicked");
    }

    let held: u64 = map.iter().map(|entry| *entry.value()).sum();
    assert_eq!(charge.owned_bytes(), held);
}
