//! The global allocator: the system allocator, but every heap block is overwritten with zeros
//! before it is given back. Copied text passes through buffers Copycraft does not own (Polars'
//! table columns, the CSV reader's chunks, hash tables, rendered grids, AppKit bridging), and
//! `zeroize` only reaches the ones it does. With this, none of them lingers in freed memory.
//! The cost is a memset per free (and a copy per `realloc`, which the system could otherwise do
//! in place).

use std::alloc::{GlobalAlloc, Layout, System};

pub struct WipeOnFree;

/// Zero `size` bytes at `ptr`. `black_box` keeps the stores: without it the compiler may drop
/// writes to memory that is freed right after.
///
/// # Safety
///
/// `ptr` must be valid for writes of `size` bytes.
unsafe fn wipe(ptr: *mut u8, size: usize) {
    unsafe { std::ptr::write_bytes(ptr, 0, size) };
    std::hint::black_box(ptr);
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

// SAFETY: every block comes from `System` with the caller's layout and goes back to it with
// that layout; `wipe` stays inside the block.
unsafe impl GlobalAlloc for WipeOnFree {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            wipe(ptr, layout.size());
            System.dealloc(ptr, layout);
        }
    }

    /// Always a new block: the system's `realloc` can move the data and free the old block
    /// without wiping it.
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe {
            let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
            let new = System.alloc(new_layout);
            if !new.is_null() {
                std::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
                self.dealloc(ptr, layout);
            }
            new
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WipeOnFree;
    use std::alloc::{GlobalAlloc, Layout};

    #[test]
    fn wipe_zeroes_the_block() {
        let mut bytes = *b"secret value";
        // SAFETY: the array is valid for its length.
        unsafe { super::wipe(bytes.as_mut_ptr(), bytes.len()) };
        assert_eq!(bytes, [0u8; 12]);
    }

    #[test]
    fn realloc_keeps_the_data_growing_and_shrinking() {
        let layout = Layout::from_size_align(16, 8).unwrap();
        // SAFETY: each block is used within its size and freed with its layout.
        unsafe {
            let ptr = WipeOnFree.alloc(layout);
            assert!(!ptr.is_null());
            for i in 0..16 {
                *ptr.add(i) = i as u8 + 1;
            }
            let grown = WipeOnFree.realloc(ptr, layout, 4096);
            assert!(!grown.is_null());
            assert_eq!(
                std::slice::from_raw_parts(grown, 16),
                &(1..=16).collect::<Vec<u8>>()[..]
            );
            let big = Layout::from_size_align(4096, 8).unwrap();
            let shrunk = WipeOnFree.realloc(grown, big, 4);
            assert_eq!(std::slice::from_raw_parts(shrunk, 4), &[1, 2, 3, 4]);
            WipeOnFree.dealloc(shrunk, Layout::from_size_align(4, 8).unwrap());
        }
    }

    #[test]
    fn programs_run_on_it() {
        // Big and small, through a Vec that grows: the program runs on it.
        let mut grown: Vec<u8> = Vec::new();
        for i in 0..100_000u32 {
            grown.push(i as u8);
        }
        assert_eq!(grown.len(), 100_000);
        assert_eq!(grown[99_999], (99_999u32 & 0xff) as u8);
    }
}
