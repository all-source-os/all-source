//! Heap profile under the `heap-profile` build: a 64 MiB allocation moves the
//! jemalloc live-byte reading, and the pprof dump is non-empty. Linux only.
#![cfg(all(feature = "heap-profile", target_os = "linux"))]

use allsource_core::infrastructure::observability::heap::{
    heap_profile_pprof, jemalloc_live_bytes,
};

#[allow(non_upper_case_globals)]
#[expect(
    unsafe_code,
    reason = "jemalloc reads malloc_conf from the binary's symbol table"
)]
#[unsafe(export_name = "malloc_conf")]
pub static malloc_conf: &[u8] = b"prof:true,prof_active:true,lg_prof_sample:19\0";

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

const BLOCK_BYTES: usize = 64 << 20;

#[tokio::test]
async fn heap_profile_nonempty() {
    let before = jemalloc_live_bytes();
    let block = vec![1u8; BLOCK_BYTES];
    std::hint::black_box(&block);
    let during = jemalloc_live_bytes();
    assert!(
        during >= before + BLOCK_BYTES as u64,
        "live bytes did not move: {before} -> {during}"
    );

    let profile = heap_profile_pprof().await.expect("pprof dump");
    assert!(!profile.is_empty(), "pprof dump is empty");
    drop(block);
}
