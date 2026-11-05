use alloc::boxed::Box;

pub fn run() {
    crate::println!("heap: start");

    create_box();
    crate::println!("  create box: ok");

    crate::println!("heap: end");
}

fn create_box() {
    let value = Box::new(24u64);
    assert_eq!(*value, 24);
}
