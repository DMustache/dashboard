use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::Ordering,
};

use crate::{
    ALLOCATED_BYTES, CURRENT_MEMORY_BYTES, DEALLOCATED_BYTES, PEAK_MEMORY_BYTES,
    REQUEST_MEMORY_DELTA,
};

pub struct TrackingAllocator;

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            let size = layout.size();
            let allocated = ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed) + size;
            let deallocated = DEALLOCATED_BYTES.load(Ordering::Relaxed);
            let current = allocated.saturating_sub(deallocated);
            CURRENT_MEMORY_BYTES.store(current, Ordering::Relaxed);

            PEAK_MEMORY_BYTES.fetch_max(current, Ordering::Relaxed);

            REQUEST_MEMORY_DELTA
                .try_with(|delta| {
                    delta.borrow_mut().0 += size;
                })
                .ok();
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size();
        System.dealloc(ptr, layout);
        let deallocated = DEALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed) + size;
        let allocated = ALLOCATED_BYTES.load(Ordering::Relaxed);
        let current = allocated.saturating_sub(deallocated);
        CURRENT_MEMORY_BYTES.store(current, Ordering::Relaxed);

        REQUEST_MEMORY_DELTA
            .try_with(|delta| {
                delta.borrow_mut().1 += size;
            })
            .ok();
    }
}
