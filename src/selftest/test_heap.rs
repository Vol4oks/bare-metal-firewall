use alloc::boxed::Box;

pub fn run() {
    create_box();
    crate::println!("heap: end");
}

fn create_box() {
    let value = Box::new(24u64);
    assert_eq!(*value, 24);
    crate::println!("  create box: ok");
}
