use core::{alloc::GlobalAlloc, cell::UnsafeCell};

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

#[global_allocator]
static HEAP: Bump = Bump {
    ptr: UnsafeCell::new(0), // ← временная заглушка, см. ниже
    end: UnsafeCell::new(0),
};

pub fn init() {
    HEAP.init(heap_start(), heap_end());
}

struct Bump {
    ptr: UnsafeCell<usize>, // текущая позиция выделения
    end: UnsafeCell<usize>, // __heap_and
}

impl Bump {
    fn init(&self, start: usize, end: usize) {
        unsafe {
            *self.ptr.get() = start;
            *self.end.get() = end;
        }
    }
}
unsafe impl Sync for Bump {}
unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let p = self.ptr.get();
        let old_p = unsafe { *p };
        let aligned = align_up(old_p, layout.align());
        let new_p = aligned + layout.size();
        if new_p > unsafe { *self.end.get() } {
            return core::ptr::null_mut();
        }

        unsafe {
            *self.ptr.get() = new_p;
            aligned as *mut u8
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

fn align_up(ptr: usize, align: usize) -> usize {
    (ptr + align - 1) & !(align - 1)
}
