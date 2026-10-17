//! The allocator of the build with threads. The standard library's
//! allocator for WebAssembly takes one spin lock for every allocation,
//! and the engine's threads allocate often enough that, past two threads,
//! a render got slower with each thread added. Here each thread keeps the
//! small blocks it frees, by size, and allocates from them without the
//! lock; only a thread whose blocks run out, or that keeps too many of one
//! size, takes it.
//!
//! A block freed on another thread than the one that allocated it joins
//! that other thread's blocks: blocks of one size are interchangeable,
//! since each was allocated from the standard allocator with the layout
//! of its size.

use std::alloc::{GlobalAlloc, Layout};
use std::ptr;

/// Block sizes: multiples of [`STEP`] up to [`SMALL`] bytes.
const STEP: usize = 16;
/// Larger blocks, or more aligned ones, go to the standard allocator.
const SMALL: usize = 1024;
const CLASSES: usize = SMALL / STEP;
/// Bytes a thread keeps of one size: at most 2 MiB in all.
const KEEP: usize = 32 << 10;

/// The blocks one thread keeps: for each size, a list linked through the
/// first word of each block.
#[derive(Debug)]
pub struct Cache {
    heads: [*mut u8; CLASSES],
    lens: [usize; CLASSES],
}

impl Cache {
    pub const fn new() -> Self {
        Self {
            heads: [ptr::null_mut(); CLASSES],
            lens: [0; CLASSES],
        }
    }

    /// # Safety
    ///
    /// As [`GlobalAlloc::alloc`]; the blocks this cache keeps come from
    /// `backend`.
    pub unsafe fn alloc(&mut self, backend: &impl GlobalAlloc, layout: Layout) -> *mut u8 {
        let Some(c) = class(layout) else {
            // SAFETY: the caller's layout, passed on.
            return unsafe { backend.alloc(layout) };
        };
        let head = self.heads[c];
        if head.is_null() {
            // SAFETY: a non-zero size, as every block of the class has.
            return unsafe { backend.alloc(class_layout(c)) };
        }
        // SAFETY: a kept block starts with the next block of its list.
        #[allow(clippy::cast_ptr_alignment)] // Blocks are aligned to `STEP`.
        let next = unsafe { head.cast::<*mut u8>().read() };
        self.heads[c] = next;
        self.lens[c] -= 1;
        head
    }

    /// # Safety
    ///
    /// As [`GlobalAlloc::dealloc`], for a block this cache, or another of
    /// the same `backend`, allocated.
    pub unsafe fn dealloc(&mut self, backend: &impl GlobalAlloc, block: *mut u8, layout: Layout) {
        let Some(c) = class(layout) else {
            // SAFETY: a large block, allocated from `backend` with this layout.
            return unsafe { backend.dealloc(block, layout) };
        };
        if self.lens[c] >= KEEP / ((c + 1) * STEP) {
            // SAFETY: a small block, allocated from `backend` with its class's layout.
            return unsafe { backend.dealloc(block, class_layout(c)) };
        }
        // SAFETY: the block is at least `STEP` bytes, aligned to `STEP`.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            block.cast::<*mut u8>().write(self.heads[c]);
        }
        self.heads[c] = block;
        self.lens[c] += 1;
    }

    /// # Safety
    ///
    /// As [`GlobalAlloc::realloc`], and as [`Self::dealloc`].
    pub unsafe fn realloc(
        &mut self,
        backend: &impl GlobalAlloc,
        block: *mut u8,
        layout: Layout,
        size: usize,
    ) -> *mut u8 {
        // SAFETY: the caller gives a size that, rounded up to the
        // alignment, does not overflow.
        let new = unsafe { Layout::from_size_align_unchecked(size, layout.align()) };
        match (class(layout), class(new)) {
            // SAFETY: a large block, allocated from `backend` with `layout`.
            (None, None) => unsafe { backend.realloc(block, layout, size) },
            (Some(a), Some(b)) if a == b => block,
            // SAFETY: both blocks are at least the shorter size long, and do not overlap.
            _ => unsafe {
                let moved = self.alloc(backend, new);
                if !moved.is_null() {
                    ptr::copy_nonoverlapping(block, moved, layout.size().min(size));
                    self.dealloc(backend, block, layout);
                }
                moved
            },
        }
    }
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

/// The size class of `layout`, when it is small.
fn class(layout: Layout) -> Option<usize> {
    (layout.size() <= SMALL && layout.align() <= STEP)
        .then(|| layout.size().max(1).div_ceil(STEP) - 1)
}

/// The layout every block of class `c` is allocated with.
fn class_layout(c: usize) -> Layout {
    // SAFETY: a non-zero multiple of 16 below 1 KiB, aligned to 16.
    unsafe { Layout::from_size_align_unchecked((c + 1) * STEP, STEP) }
}

#[cfg(all(target_family = "wasm", target_feature = "atomics"))]
mod global {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::UnsafeCell;

    use super::Cache;

    thread_local! {
        // Constant, without a destructor: a plain thread-local, which
        // allocates nothing. Threads never end, so nothing is given back.
        static CACHE: UnsafeCell<Cache> = const { UnsafeCell::new(Cache::new()) };
    }

    struct ThreadCache;

    // SAFETY: each thread's cache is used only by that thread, and only
    // in one call at a time: `System` does not call back into this.
    unsafe impl GlobalAlloc for ThreadCache {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            // SAFETY: this thread's cache, used by nothing else meanwhile.
            CACHE.with(|c| unsafe { (*c.get()).alloc(&System, layout) })
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            if super::class(layout).is_none() {
                // Fresh memory from the standard allocator is not zeroed again.
                // SAFETY: the caller's layout, passed on.
                return unsafe { System.alloc_zeroed(layout) };
            }
            // SAFETY: the caller's layout.
            let block = unsafe { self.alloc(layout) };
            if !block.is_null() {
                // SAFETY: the block is at least `layout.size()` bytes.
                unsafe { block.write_bytes(0, layout.size()) };
            }
            block
        }

        unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
            // SAFETY: as in `alloc`; the block came from this allocator.
            CACHE.with(|c| unsafe { (*c.get()).dealloc(&System, block, layout) });
        }

        unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            // SAFETY: as in `dealloc`.
            CACHE.with(|c| unsafe { (*c.get()).realloc(&System, block, layout, size) })
        }
    }

    #[global_allocator]
    static ALLOCATOR: ThreadCache = ThreadCache;
}

#[cfg(test)]
mod tests {
    use std::alloc::System;

    use super::*;

    fn layout(size: usize, align: usize) -> Layout {
        Layout::from_size_align(size, align).unwrap()
    }

    #[test]
    fn a_freed_small_block_is_reused_for_its_size() {
        let mut cache = Cache::new();
        unsafe {
            let a = cache.alloc(&System, layout(20, 8));
            cache.dealloc(&System, a, layout(20, 8));
            // 20 and 32 bytes are one size; 48 is another.
            assert_eq!(cache.alloc(&System, layout(32, 16)), a);
            let b = cache.alloc(&System, layout(48, 4));
            assert_ne!(b, a);
            cache.dealloc(&System, a, layout(32, 16));
            cache.dealloc(&System, b, layout(48, 4));
            assert_eq!(cache.lens.iter().sum::<usize>(), 2);
        }
    }

    #[test]
    fn large_or_aligned_blocks_are_not_kept() {
        let mut cache = Cache::new();
        unsafe {
            for l in [layout(SMALL + 1, 8), layout(64, 32)] {
                let a = cache.alloc(&System, l);
                a.write_bytes(7, l.size());
                cache.dealloc(&System, a, l);
            }
        }
        assert_eq!(cache.lens.iter().sum::<usize>(), 0);
    }

    #[test]
    fn a_thread_keeps_a_bounded_number_of_blocks() {
        let mut cache = Cache::new();
        let l = layout(SMALL, 8);
        unsafe {
            let blocks: Vec<_> = (0..100).map(|_| cache.alloc(&System, l)).collect();
            for b in blocks {
                cache.dealloc(&System, b, l);
            }
        }
        assert_eq!(cache.lens[CLASSES - 1], KEEP / SMALL);
    }

    #[test]
    fn realloc_keeps_the_bytes_across_sizes() {
        let mut cache = Cache::new();
        unsafe {
            let mut l = layout(10, 1);
            let mut p = cache.alloc(&System, l);
            for i in 0..10 {
                p.add(i).write(i as u8);
            }
            // Within one size, across sizes, to a large block and back.
            for size in [16, 100, 5000, 12] {
                p = cache.realloc(&System, p, l, size);
                l = layout(size, 1);
                for i in 0..10 {
                    assert_eq!(p.add(i).read(), i as u8);
                }
            }
            cache.dealloc(&System, p, l);
        }
    }

    #[test]
    fn sizes_round_up_to_their_class() {
        assert_eq!(class(layout(0, 1)), Some(0));
        assert_eq!(class(layout(16, 16)), Some(0));
        assert_eq!(class(layout(17, 1)), Some(1));
        assert_eq!(class(layout(SMALL, 16)), Some(CLASSES - 1));
        assert_eq!(class(layout(SMALL + 1, 1)), None);
    }
}
