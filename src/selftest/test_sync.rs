use crate::sync::SpinLock;

pub fn run() {
    try_lock_basics();
    lock_data();
    compile_time_traits();
    crate::println!("sync: end");
}

fn try_lock_basics() {
    let lock = SpinLock::new(0u32);

    // Свободный lock
    assert!(lock.try_lock().is_some(), "fresh lock must be free");

    // Занятый lock
    let guard = lock.lock();
    assert!(lock.try_lock().is_none(), "held lock must reject try_lock");
    drop(guard);

    // После drop снова свободен
    assert!(lock.try_lock().is_some(), "drop must release");

    // Много циклов без зависания
    for _ in 0..1000 {
        let _ = lock.lock();
    }

    crate::println!("  try_lock: ok");
}

fn lock_data() {
    let lock = SpinLock::new(0u32);

    *lock.lock() = 42;
    assert_eq!(*lock.lock(), 42);

    crate::println!("  data: ok");
}

fn compile_time_traits() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SpinLock<u32>>();
    crate::println!("  Send + Sync: ok");
}
