//! Threads on the build with shared memory (`inkoxide-threads.wasm`,
//! `scripts/build-wasm.sh --threads`). WebAssembly cannot start a thread:
//! JavaScript starts Web Workers, each with an instance of the module on
//! the same memory, and each runs one of rayon's threads.
//!
//! 1. For each worker, the render thread calls [`ink_thread_alloc`], which
//!    gives the worker a block: a stack below the pointer it returns, and
//!    room for its thread-locals above. The worker sets its
//!    `__stack_pointer` there and calls `__wasm_init_tls` with it, before
//!    any Rust code runs on it, then calls [`ink_thread_run`], which waits.
//! 2. Once every worker has started, [`ink_threads_start`] builds rayon's
//!    global pool, whose threads it queues instead of spawning; each
//!    waiting worker takes one and runs it for the life of the instance.
//! 3. Renders use that pool unless they ask for one thread, which runs on
//!    the render thread ([`engine_threads`]).
//!
//! A panic on any thread aborts the whole engine: JavaScript ends every
//! worker, since the memory they share may be inconsistent.

use std::alloc::Layout;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};

/// The stack of each pool thread. Natively, the deepest document the
/// engine accepts fits 2 MiB (`crates/core/tests/limits.rs`).
pub const THREAD_STACK: usize = 4 << 20;

/// Most threads of a pool, as natively.
pub const MAX_THREADS: u32 = 256;

/// Threads of the pool, 0 until it is started.
static POOL: AtomicU32 = AtomicU32::new(0);

/// Threads a render may ask for: [`MAX_THREADS`] once there is a pool,
/// and otherwise 1.
pub fn max_threads() -> u32 {
    if POOL.load(Ordering::Acquire) > 1 {
        MAX_THREADS
    } else {
        1
    }
}

/// The engine's `threads` for a render that asked for `asked`: one
/// thread when it asked for one or there is no pool, and otherwise
/// rayon's global pool (`None`), the pool this module starts. Without a
/// pool a render must not use the global pool: rayon would make one of
/// the calling thread, and [`ink_threads_start`] could not build it.
pub fn engine_threads(asked: Option<usize>) -> Option<usize> {
    if !cfg!(target_feature = "atomics") {
        return asked;
    }
    match asked {
        Some(1) => Some(1),
        _ if POOL.load(Ordering::Acquire) > 1 => None,
        _ => Some(1),
    }
}

/// Items that threads wait for.
pub struct Queue<T> {
    items: Mutex<VecDeque<T>>,
    added: Condvar,
}

impl<T> Queue<T> {
    pub const fn new() -> Self {
        Self {
            items: Mutex::new(VecDeque::new()),
            added: Condvar::new(),
        }
    }

    pub fn push(&self, item: T) {
        // A panic aborts the module, so a lock is never poisoned there.
        let mut items = self.items.lock().unwrap_or_else(PoisonError::into_inner);
        items.push_back(item);
        self.added.notify_one();
    }

    /// The oldest item, once there is one.
    pub fn pop(&self) -> T {
        let mut items = self.items.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(item) = items.pop_front() {
                return item;
            }
            items = self
                .added
                .wait(items)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// The block of one pool thread whose thread-locals take `tls_size`
/// bytes aligned to `tls_align`: [`THREAD_STACK`] bytes of stack, then
/// the thread-locals, which start at offset [`THREAD_STACK`]. `None` for
/// an alignment that is not a power of two or does not divide the stack.
pub fn block_layout(tls_size: usize, tls_align: usize) -> Option<Layout> {
    if !tls_align.max(1).is_power_of_two() {
        return None;
    }
    let align = tls_align.max(16);
    if !THREAD_STACK.is_multiple_of(align) {
        return None;
    }
    Layout::from_size_align(THREAD_STACK.checked_add(tls_size)?, align).ok()
}

#[cfg(all(target_family = "wasm", target_feature = "atomics"))]
mod abi {
    use std::sync::atomic::Ordering;

    use inkoxide_binding::Failure;

    use super::{block_layout, Queue, MAX_THREADS, POOL, THREAD_STACK};

    /// The threads rayon asked for, which workers take.
    static THREADS: Queue<rayon::ThreadBuilder> = Queue::new();

    /// A block for one pool thread (see [`block_layout`]): the pointer is
    /// the top of its stack and the start of its thread-locals. Never
    /// freed: the thread runs for the life of the instance.
    #[unsafe(no_mangle)]
    pub extern "C" fn ink_thread_alloc(tls_size: usize, tls_align: usize) -> *mut u8 {
        let Some(layout) = block_layout(tls_size, tls_align) else {
            return std::ptr::null_mut();
        };
        // SAFETY: the layout has a non-zero size.
        let base = unsafe { std::alloc::alloc(layout) };
        if base.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        // SAFETY: the block is longer than the stack.
        unsafe { base.add(THREAD_STACK) }
    }

    /// Builds the pool of `n` threads; the result is empty or an error.
    /// Workers waiting in [`ink_thread_run`] run its threads.
    #[unsafe(no_mangle)]
    pub extern "C" fn ink_threads_start(n: u32) -> u32 {
        let started = (2..=MAX_THREADS)
            .contains(&n)
            .then(|| {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(n as usize)
                    .spawn_handler(|thread| {
                        THREADS.push(thread);
                        Ok(())
                    })
                    .build_global()
            })
            .ok_or_else(|| {
                Failure::invalid_arg(
                    "threads",
                    &format!("must be an integer from 2 to {MAX_THREADS}"),
                )
            })
            .and_then(|r| r.map_err(|e| Failure::internal(&format!("thread pool: {e}"))));
        if started.is_ok() {
            POOL.store(n, Ordering::Release);
        }
        crate::finish(started.map(|()| Vec::new()))
    }

    /// Runs one of the pool's threads on the calling worker, once
    /// [`ink_threads_start`] has queued it; it does not return.
    #[unsafe(no_mangle)]
    pub extern "C" fn ink_thread_run() {
        THREADS.pop().run();
    }

    /// Panics on a thread of the pool, so that the tests can check that
    /// the JavaScript side ends the engine. The call never returns. Not
    /// part of the public API.
    #[doc(hidden)]
    #[unsafe(no_mangle)]
    pub extern "C" fn ink_test_pool_panic() {
        // Called from outside the pool, rayon runs the scope on a thread
        // of it.
        rayon::scope(|_| panic!("test panic"));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn queue_hands_items_to_waiting_threads_in_order() {
        let queue = Arc::new(Queue::new());
        let waiting: Vec<_> = (0..4)
            .map(|_| {
                let q = queue.clone();
                std::thread::spawn(move || q.pop())
            })
            .collect();
        for i in 0..4 {
            queue.push(i);
        }
        let mut got: Vec<i32> = waiting.into_iter().map(|t| t.join().unwrap()).collect();
        got.sort_unstable();
        assert_eq!(got, [0, 1, 2, 3]);

        for i in 10..13 {
            queue.push(i);
        }
        assert_eq!([queue.pop(), queue.pop(), queue.pop()], [10, 11, 12]);
    }

    #[test]
    fn block_puts_the_thread_locals_above_the_stack() {
        let layout = block_layout(100, 8).unwrap();
        assert_eq!(layout.size(), THREAD_STACK + 100);
        assert_eq!(layout.align(), 16);
        assert_eq!(block_layout(0, 64).unwrap().align(), 64);
        assert_eq!(block_layout(0, 0).unwrap().align(), 16);
        assert!(block_layout(0, 3).is_none());
        assert!(block_layout(0, THREAD_STACK * 2).is_none());
        assert!(block_layout(usize::MAX, 8).is_none());
    }

    #[test]
    fn without_atomics_a_render_keeps_its_threads() {
        // Native builds have no atomics target feature: nothing changes.
        assert_eq!(max_threads(), 1);
        assert_eq!(engine_threads(None), None);
        assert_eq!(engine_threads(Some(1)), Some(1));
    }
}
