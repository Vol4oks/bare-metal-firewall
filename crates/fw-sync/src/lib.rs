#![cfg_attr(not(test), no_std)]

use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        SpinLockGuard { lock: self }
    }

    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinLockGuard { lock: self })
        } else {
            None
        }
    }
}

#[must_use = "if unused, the lock is released immediately"]
pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}
impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    #[test]
    fn four_threads_ten_thousand() {
        let lock = Arc::new(SpinLock::new(0u32));
        let mut hs = Vec::new();
        for _ in 0..4 {
            let l = Arc::clone(&lock);
            hs.push(std::thread::spawn(move || {
                for _ in 0..10_000 {
                    *l.lock() += 1;
                }
            }));
        }
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(*lock.lock(), 40_000);
    }

    #[test]
    fn try_lock_contract() {
        let lock = SpinLock::new(0u32);
        assert!(lock.try_lock().is_some(), "fresh lock must be free");
        let guard = lock.lock();
        assert!(lock.try_lock().is_none(), "held lock must reject try_lock");
        drop(guard);
        assert!(lock.try_lock().is_some(), "drop must release");
    }

    #[test]
    fn try_lock_from_other_thread() {
        let lock = Arc::new(SpinLock::new(0u32));
        let guard = lock.lock();

        let (tx, rx) = mpsc::channel();
        let l = Arc::clone(&lock);
        let h = std::thread::spawn(move || {
            tx.send(l.try_lock().is_none()).unwrap();
        });

        assert!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            "other thread must see lock held"
        );
        h.join().unwrap();

        drop(guard);
        assert!(lock.try_lock().is_some());
    }

    #[test]
    fn lock_blocks_until_released() {
        let lock = Arc::new(SpinLock::new(0u32));
        let guard = lock.lock();

        let (tx, rx) = mpsc::channel();
        let l = Arc::clone(&lock);
        let h = std::thread::spawn(move || {
            let _g = l.lock();
            tx.send(()).unwrap();
        });

        assert!(
            rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "lock() must block while held"
        );
        drop(guard);
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_ok(),
            "lock() must acquire after release"
        );
        h.join().unwrap();
    }

    #[test]
    fn write_and_read() {
        let lock = SpinLock::new(0u32);
        *lock.lock() = 42;
        assert_eq!(*lock.lock(), 42);
    }

    #[test]
    fn traits() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SpinLock<u32>>();
    }
}
