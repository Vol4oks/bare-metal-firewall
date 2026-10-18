use alloc::boxed::Box;

pub fn run() {
    let value = Box::new(24u64);
    assert_eq!(*value, 24);
    crate::println!("  heap: ok");
}
