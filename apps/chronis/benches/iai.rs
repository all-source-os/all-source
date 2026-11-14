//! iai-callgrind deterministic instruction-count benchmarks (Linux/valgrind only).
//! Compiles on macOS via `cargo check --bench iai`; runs in CI via perf-bench.yml.

#[cfg(not(feature = "dhat-heap"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::sync::Arc;

use allsource_core::embedded::{Config, EmbeddedCore};
use chronis::{
    domain::{repository::TaskRepository, task::TaskType},
    infrastructure::{
        backend::CoreBackend, core_task_repo::CoreTaskRepository, projection::TaskProjection,
    },
};
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use tokio::runtime::Runtime;

fn rt() -> Runtime {
    Runtime::new().expect("tokio runtime")
}

async fn setup() -> CoreTaskRepository {
    let config = Config::builder()
        .single_tenant(true)
        .build()
        .expect("config");
    let core = Arc::new(EmbeddedCore::open(config).await.expect("core"));
    core.inner()
        .register_projection_with_backfill(
            &(Arc::new(TaskProjection::new()) as Arc<dyn allsource_core::application::Projection>),
        )
        .expect("projection");
    let backend = Arc::new(CoreBackend::new_embedded(core));
    CoreTaskRepository::new(backend)
}

async fn populated(n: usize) -> CoreTaskRepository {
    let repo = setup().await;
    for i in 0..n {
        repo.create_task(
            &format!("t-{i:08x}"),
            &format!("task {i}"),
            "p2",
            &[],
            TaskType::Task,
            None,
            None,
        )
        .await
        .unwrap();
    }
    repo
}

// Every future passed to `block_on` is a module-level `async fn`. The default
// entry point toggles collection on any symbol under the benchmark's wrapper
// module, and `block_on::<{closure}>` of a closure defined in a benchmark is
// one, so when it is not inlined everything it runs goes uncounted.
#[library_benchmark]
fn bench_workspace_init() {
    let rt = rt();
    std::hint::black_box(rt.block_on(setup()));
}

#[library_benchmark]
#[bench::small(10)]
#[bench::medium(100)]
fn bench_create_batch(n: usize) {
    let rt = rt();
    std::hint::black_box(rt.block_on(populated(n)));
}

#[library_benchmark]
fn bench_list_after_create() {
    let rt = rt();
    let repo = rt.block_on(populated(100));
    std::hint::black_box(repo.list_tasks(None).unwrap());
}

library_benchmark_group!(
    name = chronis_hot_path;
    benchmarks = bench_workspace_init, bench_create_batch, bench_list_after_create
);

main!(library_benchmark_groups = chronis_hot_path);
