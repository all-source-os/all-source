//! iai-callgrind deterministic instruction-count benchmarks (Linux/valgrind only).
//! Compiles on macOS via `cargo check --bench iai`; runs in CI via perf-bench.yml.

#[cfg(not(feature = "dhat-heap"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use allsource_core::prime::Prime;
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use serde_json::json;
use tokio::runtime::Runtime;

fn rt() -> Runtime {
    Runtime::new().expect("tokio runtime")
}

#[library_benchmark]
fn bench_open_in_memory() {
    let rt = rt();
    std::hint::black_box(rt.block_on(Prime::open_in_memory()).unwrap());
}

#[library_benchmark]
#[bench::small(100)]
#[bench::medium(1_000)]
fn bench_add_node_batch(n: usize) {
    let rt = rt();
    rt.block_on(async {
        let prime = Prime::open_in_memory().await.unwrap();
        for i in 0..n {
            std::hint::black_box(
                prime
                    .add_node("person", json!({ "name": format!("p{i}"), "index": i }))
                    .await
                    .unwrap(),
            );
        }
    });
}

fn populated_graph() -> (Runtime, Prime) {
    let rt = rt();
    let prime = rt.block_on(async {
        let p = Prime::open_in_memory().await.unwrap();
        for i in 0..1_000usize {
            p.add_node("person", json!({ "index": i })).await.unwrap();
        }
        p
    });
    (rt, prime)
}

// Population and teardown run on runtime threads whose scheduling varies
// between runs, so they stay outside the measured frame.
#[library_benchmark]
#[bench::thousand_nodes(setup = populated_graph)]
fn bench_stats_over_populated_graph(graph: (Runtime, Prime)) -> (Runtime, Prime) {
    std::hint::black_box(graph.1.stats());
    graph
}

library_benchmark_group!(
    name = prime_hot_path;
    benchmarks = bench_open_in_memory, bench_add_node_batch, bench_stats_over_populated_graph
);

main!(library_benchmark_groups = prime_hot_path);
