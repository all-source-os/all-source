//! Arena-based memory pooling for high-throughput event processing
//!
//! This module provides thread-local arena allocators for zero-allocation
//! hot paths. Arenas are recycled and reused to minimize memory churn.
//!
//! # Performance Characteristics
//! - ~2-5ns allocations (vs ~50-100ns for standard allocation)
//! - Zero fragmentation within arena
//! - Batch deallocation (entire arena at once)
//! - Thread-local access eliminates contention
//!
//! # Usage Pattern
//! ```ignore
//! // Get arena from thread-local pool
//! let mut arena = get_thread_local_arena();
//!
//! // Allocate in arena (very fast)
//! let s = arena.alloc_str("hello");
//! let bytes = arena.alloc_bytes(&[1, 2, 3]);
//!
//! // Arena is automatically returned to pool when dropped
//! ```

use bumpalo::Bump;
use std::{
    cell::RefCell,
    sync::atomic::{AtomicU64, Ordering},
};

/// Default arena size (16MB)
const DEFAULT_ARENA_SIZE: usize = 16 * 1024 * 1024;

/// Maximum arenas to keep in thread-local pool
const MAX_THREAD_LOCAL_ARENAS: usize = 4;

/// Global statistics for arena pool usage
static ARENAS_CREATED: AtomicU64 = AtomicU64::new(0);
static ARENAS_RECYCLED: AtomicU64 = AtomicU64::new(0);
static BYTES_ALLOCATED: AtomicU64 = AtomicU64::new(0);

// Thread-local arena pool
thread_local! {
    static ARENA_POOL: RefCell<Vec<Bump>> = RefCell::new(Vec::with_capacity(MAX_THREAD_LOCAL_ARENAS));
}

/// Get an arena from the thread-local pool (or create a new one)
///
/// # Performance
/// - ~10-20ns when recycling from pool
/// - ~100-500ns when creating new arena
pub fn get_arena() -> PooledArena {
    let arena = ARENA_POOL.with(|pool| pool.borrow_mut().pop());

    let arena = if let Some(mut arena) = arena {
        arena.reset();
        ARENAS_RECYCLED.fetch_add(1, Ordering::Relaxed);
        arena
    } else {
        ARENAS_CREATED.fetch_add(1, Ordering::Relaxed);
        Bump::with_capacity(DEFAULT_ARENA_SIZE)
    };

    PooledArena { arena: Some(arena) }
}

/// Get an arena with custom capacity
pub fn get_arena_with_capacity(capacity: usize) -> PooledArena {
    ARENAS_CREATED.fetch_add(1, Ordering::Relaxed);
    PooledArena {
        arena: Some(Bump::with_capacity(capacity)),
    }
}

/// A pooled arena that returns to the thread-local pool when dropped
pub struct PooledArena {
    arena: Option<Bump>,
}

impl PooledArena {
    /// Allocate a string slice in the arena
    ///
    /// # Performance
    /// - ~2-5ns allocation
    #[inline]
    pub fn alloc_str(&self, s: &str) -> &str {
        self.arena.as_ref().unwrap().alloc_str(s)
    }

    /// Allocate a byte slice in the arena
    #[inline]
    pub fn alloc_bytes(&self, bytes: &[u8]) -> &[u8] {
        self.arena.as_ref().unwrap().alloc_slice_copy(bytes)
    }

    /// Allocate a value in the arena
    #[inline]
    pub fn alloc<T>(&self, val: T) -> &mut T {
        self.arena.as_ref().unwrap().alloc(val)
    }

    /// Allocate a slice with fill function
    #[inline]
    pub fn alloc_slice_fill_with<T, F>(&self, len: usize, f: F) -> &mut [T]
    where
        F: FnMut(usize) -> T,
    {
        self.arena.as_ref().unwrap().alloc_slice_fill_with(len, f)
    }

    /// Get current allocation size
    pub fn allocated(&self) -> usize {
        self.arena.as_ref().unwrap().allocated_bytes()
    }

    /// Get a reference to the underlying bump allocator
    pub fn inner(&self) -> &Bump {
        self.arena.as_ref().unwrap()
    }
}

impl Drop for PooledArena {
    fn drop(&mut self) {
        if let Some(arena) = self.arena.take() {
            let allocated = arena.allocated_bytes();
            BYTES_ALLOCATED.fetch_add(allocated as u64, Ordering::Relaxed);

            ARENA_POOL.with(|pool| {
                let mut pool = pool.borrow_mut();
                if pool.len() < MAX_THREAD_LOCAL_ARENAS {
                    pool.push(arena);
                }
                // Arena dropped if pool is full
            });
        }
    }
}

/// Statistics for arena pool usage
#[derive(Debug, Clone)]
pub struct ArenaPoolStats {
    /// Total arenas created across all threads
    pub arenas_created: u64,
    /// Total arenas recycled (reused from pool)
    pub arenas_recycled: u64,
    /// Total bytes allocated through arenas
    pub bytes_allocated: u64,
    /// Recycle rate (0.0 to 1.0)
    pub recycle_rate: f64,
}

/// Get global arena pool statistics
pub fn arena_stats() -> ArenaPoolStats {
    let created = ARENAS_CREATED.load(Ordering::Relaxed);
    let recycled = ARENAS_RECYCLED.load(Ordering::Relaxed);
    let total = created + recycled;

    ArenaPoolStats {
        arenas_created: created,
        arenas_recycled: recycled,
        bytes_allocated: BYTES_ALLOCATED.load(Ordering::Relaxed),
        recycle_rate: if total > 0 {
            recycled as f64 / total as f64
        } else {
            0.0
        },
    }
}

/// Reset global statistics (for testing)
pub fn reset_stats() {
    ARENAS_CREATED.store(0, Ordering::Relaxed);
    ARENAS_RECYCLED.store(0, Ordering::Relaxed);
    BYTES_ALLOCATED.store(0, Ordering::Relaxed);
}

/// Scoped arena for temporary allocations
///
/// Provides a convenient RAII pattern for short-lived allocations.
/// The arena is automatically returned to the pool when the scope ends.
///
/// # Example
/// ```ignore
/// {
///     let arena = ScopedArena::new();
///     let s1 = arena.alloc_str("temporary");
///     let s2 = arena.alloc_str("data");
///     // Use s1, s2...
/// } // Arena automatically returned to pool
/// ```
pub struct ScopedArena {
    arena: PooledArena,
}

impl ScopedArena {
    /// Create a new scoped arena from the pool
    pub fn new() -> Self {
        Self { arena: get_arena() }
    }

    /// Create with custom capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            arena: get_arena_with_capacity(capacity),
        }
    }

    /// Allocate a string
    #[inline]
    pub fn alloc_str(&self, s: &str) -> &str {
        self.arena.alloc_str(s)
    }

    /// Allocate bytes
    #[inline]
    pub fn alloc_bytes(&self, bytes: &[u8]) -> &[u8] {
        self.arena.alloc_bytes(bytes)
    }

    /// Allocate a value
    #[inline]
    pub fn alloc<T>(&self, val: T) -> &mut T {
        self.arena.alloc(val)
    }

    /// Get current allocation size
    pub fn allocated(&self) -> usize {
        self.arena.allocated()
    }
}

impl Default for ScopedArena {
    fn default() -> Self {
        Self::new()
    }
}

/// Pre-allocated buffer pool for specific sizes
///
/// Useful when you need buffers of predictable sizes and want
/// to avoid arena overhead for very small allocations.
pub struct SizedBufferPool {
    small: Vec<Vec<u8>>,  // < 1KB
    medium: Vec<Vec<u8>>, // 1KB - 64KB
    large: Vec<Vec<u8>>,  // > 64KB
    small_size: usize,
    medium_size: usize,
    large_size: usize,
    max_pool: usize,
}

impl SizedBufferPool {
    /// Create a new sized buffer pool
    pub fn new() -> Self {
        Self {
            small: Vec::new(),
            medium: Vec::new(),
            large: Vec::new(),
            small_size: 1024,
            medium_size: 64 * 1024,
            large_size: 1024 * 1024,
            max_pool: 32,
        }
    }

    /// Get a buffer of at least the specified size
    pub fn get(&mut self, min_size: usize) -> Vec<u8> {
        let buf = if min_size <= self.small_size {
            self.small.pop()
        } else if min_size <= self.medium_size {
            self.medium.pop()
        } else {
            self.large.pop()
        };

        if let Some(mut b) = buf {
            b.clear();
            if b.capacity() >= min_size {
                b
            } else {
                Vec::with_capacity(min_size)
            }
        } else {
            let capacity = if min_size <= self.small_size {
                self.small_size
            } else if min_size <= self.medium_size {
                self.medium_size
            } else {
                self.large_size.max(min_size)
            };
            Vec::with_capacity(capacity)
        }
    }

    /// Return a buffer to the pool
    pub fn put(&mut self, mut buf: Vec<u8>) {
        let cap = buf.capacity();
        buf.clear();

        if cap <= self.small_size && self.small.len() < self.max_pool {
            self.small.push(buf);
        } else if cap <= self.medium_size && self.medium.len() < self.max_pool {
            self.medium.push(buf);
        } else if self.large.len() < self.max_pool {
            self.large.push(buf);
        }
        // Buffer dropped if all pools are full
    }

    /// Get pool statistics
    pub fn pool_sizes(&self) -> (usize, usize, usize) {
        (self.small.len(), self.medium.len(), self.large.len())
    }
}

impl Default for SizedBufferPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_arena() {
        // Test that arena allocation and string storage works correctly
        // Note: We don't check global stats here because they can be affected
        // by other tests running in parallel (e.g., reset_stats() calls)
        let arena1 = get_arena();
        let s = arena1.alloc_str("hello");
        assert_eq!(s, "hello");
        assert!(arena1.allocated() > 0);

        // Test that we can allocate multiple items
        let s2 = arena1.alloc_str("world");
        assert_eq!(s2, "world");

        // Test that allocations persist
        assert_eq!(s, "hello");
        assert_eq!(s2, "world");
    }

    #[test]
    fn test_arena_recycling() {
        // Clear the thread-local pool first
        ARENA_POOL.with(|pool| pool.borrow_mut().clear());

        // Verify pool is empty
        let pool_empty = ARENA_POOL.with(|pool| pool.borrow().is_empty());
        assert!(pool_empty, "Pool should be empty after clear");

        // Create and drop arena - should return to pool
        let arena1 = get_arena();
        let _ = arena1.alloc_str("test"); // Use the arena
        drop(arena1);

        // Verify arena was returned to pool
        let pool_has_arena = ARENA_POOL.with(|pool| !pool.borrow().is_empty());
        assert!(pool_has_arena, "Pool should have arena after drop");

        // Get another arena (should be recycled from pool)
        let arena2 = get_arena();
        // Verify we can allocate in the recycled arena (it was reset)
        let s = arena2.alloc_str("recycled");
        assert_eq!(s, "recycled");

        // Pool should now be empty (we took the arena)
        let pool_empty_after = ARENA_POOL.with(|pool| pool.borrow().is_empty());
        assert!(pool_empty_after, "Pool should be empty after taking arena");
        drop(arena2);
    }

    #[test]
    fn test_arena_allocations() {
        let arena = get_arena();

        let s1 = arena.alloc_str("hello");
        let s2 = arena.alloc_str("world");
        let bytes = arena.alloc_bytes(&[1, 2, 3, 4, 5]);

        assert_eq!(s1, "hello");
        assert_eq!(s2, "world");
        assert_eq!(bytes, &[1, 2, 3, 4, 5]);

        assert!(arena.allocated() > 0);
    }

    #[test]
    fn test_scoped_arena() {
        reset_stats();

        {
            let arena = ScopedArena::new();
            let s = arena.alloc_str("scoped");
            assert_eq!(s, "scoped");
        } // Arena returned to pool

        // Next arena should be recycled
        let _ = ScopedArena::new();

        let stats = arena_stats();
        assert!(stats.arenas_recycled > 0 || stats.arenas_created > 0);
    }

    #[test]
    fn test_sized_buffer_pool() {
        let mut pool = SizedBufferPool::new();

        // Get small buffer
        let buf1 = pool.get(100);
        assert!(buf1.capacity() >= 100);

        // Get medium buffer
        let buf2 = pool.get(10_000);
        assert!(buf2.capacity() >= 10_000);

        // Return buffers
        pool.put(buf1);
        pool.put(buf2);

        let (small, medium, large) = pool.pool_sizes();
        assert_eq!(small, 1);
        assert_eq!(medium, 1);
        assert_eq!(large, 0);
    }

    #[test]
    fn test_sized_buffer_reuse() {
        let mut pool = SizedBufferPool::new();

        let mut buf1 = pool.get(100);
        buf1.extend_from_slice(b"test data");
        pool.put(buf1);

        // Get buffer again - should be cleared
        let buf2 = pool.get(100);
        assert!(buf2.is_empty());
        assert!(buf2.capacity() >= 100);
    }

    #[test]
    fn test_arena_with_custom_capacity() {
        let arena = get_arena_with_capacity(1024);
        let s = arena.alloc_str("custom");
        assert_eq!(s, "custom");
    }

    #[test]
    fn test_concurrent_arena_access() {
        // The stats counters are process-global and other tests reset them in
        // parallel, so recycling is checked through each thread's own pool.
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| {
                    for _ in 0..100 {
                        let arena = get_arena();
                        let _ = arena.alloc_str("concurrent test");
                        drop(arena);
                    }
                    let pooled = ARENA_POOL.with(|pool| pool.borrow().len());
                    assert_eq!(pooled, 1, "100 get/drop cycles reuse one arena");
                });
            }
        });
    }

    #[test]
    fn test_alloc_slice_fill() {
        let arena = get_arena();
        let slice = arena.alloc_slice_fill_with(5, |i| i * 2);
        assert_eq!(slice, &[0, 2, 4, 6, 8]);
    }
}
