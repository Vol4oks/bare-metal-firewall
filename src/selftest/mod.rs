mod test_heap;
mod test_sync;

pub fn run() {
    crate::println!("selftest: start");
    test_sync::run();
    test_heap::run();
    crate::println!("selftest: end");
}
