use core::alloc::{GlobalAlloc, Layout};

use crate::sync::SpinLock;

unsafe extern "C" {
    static __heap_start: u8;
    static __heap_end: u8;
}

pub fn heap_start() -> usize {
    &raw const __heap_start as usize
}
pub fn heap_end() -> usize {
    &raw const __heap_end as usize
}

struct BumpState {
    ptr: usize,
    end: usize,
}

impl BumpState {
    const fn new() -> Self {
        Self { ptr: 0, end: 0 }
    }

    fn init(&mut self, start: usize, end: usize) {
        self.ptr = start;
        self.end = end;
    }

    fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let Some(aligned) = align_up(self.ptr, layout.align()) else {
            return core::ptr::null_mut();
        };
        let Some(new_p) = aligned.checked_add(layout.size()) else {
            return core::ptr::null_mut();
        };
        if new_p > self.end {
            return core::ptr::null_mut();
        }
        self.ptr = new_p;
        aligned as *mut u8
    }
}

struct Bump {
    state: SpinLock<BumpState>,
}

impl Bump {
    const fn new() -> Self {
        Self {
            state: SpinLock::new(BumpState::new()),
        }
    }

    fn init(&self, start: usize, end: usize) {
        let mut state = self.state.lock();
        state.init(start, end);
    }

    fn alloc_inner(&self, layout: Layout) -> *mut u8 {
        let mut state = self.state.lock();
        state.alloc(layout)
    }
}

#[global_allocator]
static HEAP: Bump = Bump::new();

pub fn init() {
    HEAP.init(heap_start(), heap_end());
}

unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        self.alloc_inner(layout)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

fn align_up(ptr: usize, align: usize) -> Option<usize> {
    ptr.checked_add(align - 1).map(|p| p & !(align - 1))
}
