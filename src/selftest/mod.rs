mod test_heap;
mod test_sched;
mod test_sync;

pub fn run() {
    crate::println!("\nselftest: start");
    test_sync::run();
    test_heap::run();
    test_sched::run();
    crate::println!("selftest: end");
}
