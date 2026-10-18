mod test_heap;
mod test_sync;

pub fn run() {
    test_sync::run();
    test_heap::run();
}
