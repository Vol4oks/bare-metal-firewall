---
tags: [тестирование, no_std, spinlock, этап-4]
aliases: [no_std tests, selftest]
stage: 4
---

# Тестирование no_std: selftest в QEMU и host-тесты

> [!abstract] Суть
> `cargo test` для `aarch64-unknown-none` не работает «из коробки»: тестовому харнессу нужен `test`/`std`. Практичные варианты: (1) `selftest`-модуль, который запускается в QEMU по feature; (2) вынести чистую логику (`SpinLock`) в отдельный `no_std`-крейт и гонять host-тесты с `std::thread`.

## Почему `cargo test` не работает для бинарника

- `#[test]` требует test harness (`test` crate), которого нет для bare-metal target.
- `#![no_std]`/`#![no_main]`, asm, `linker.ld` не собираются для host.
- `custom_test_frameworks` — nightly; для учебного проекта не нужен.

## Вариант A: selftest в QEMU

Добавить feature в `Cargo.toml`:

```toml
[features]
selftest = []
```

`src/selftest.rs`:

```rust
use crate::sync::SpinLock;

pub fn run() {
    crate::println!("selftest: start");
    try_lock_basics();
    lock_data();
    compile_time_traits();
    crate::println!("selftest: OK");
}

fn try_lock_basics() {
    let lock = SpinLock::new(0u32);

    // Свободен
    assert!(lock.try_lock().is_some());

    // Занят: второй захват должен вернуть None
    let guard = lock.lock();
    assert!(lock.try_lock().is_none());
    drop(guard);

    // После drop снова свободен
    assert!(lock.try_lock().is_some());

    // Много циклов lock/drop без зависания
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
```

В `main.rs`:

```rust
#[cfg(feature = "selftest")]
mod selftest;

// ...
#[cfg(feature = "selftest")]
selftest::run();
```

Запуск:

```bash
cargo run --features selftest
```

Ожидаемо:

```text
selftest: start
  try_lock: ok
  data: ok
  Send + Sync: ok
selftest: OK
```

Если assert не прошёл — сработает `panic_handler`, и в UART будет сообщение.

> [!warning] Guard — временный объект
> `assert!(lock.try_lock().is_some())` захватывает lock и сразу отпускает его в конце выражения. Если хочешь удерживать — сохрани guard в переменную.

## Вариант B: host-тесты `SpinLock`

`SpinLock` не зависит от архитектуры: только `core::sync::atomic`, `UnsafeCell`, `spin_loop`. Его можно вынести в отдельный `no_std`-крейт и тестировать на хосте настоящими потоками.

```text
Cargo.toml            # workspace
sync/
  Cargo.toml
  src/lib.rs
bare-metal-firewall/  # текущий бинарник
```

`sync/src/lib.rs`:

```rust
#![cfg_attr(not(test), no_std)]

// ... SpinLock, SpinLockGuard ...

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn try_lock_while_held() {
        let lock = SpinLock::new(0u32);
        let guard = lock.lock();
        assert!(lock.try_lock().is_none());
        drop(guard);
        assert!(lock.try_lock().is_some());
    }

    #[test]
    fn mutual_exclusion_counter() {
        let lock = Arc::new(SpinLock::new(0u64));
        let mut handles = Vec::new();

        for _ in 0..4 {
            let lock = Arc::clone(&lock);
            handles.push(thread::spawn(move || {
                for _ in 0..10_000 {
                    *lock.lock() += 1;
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(*lock.lock(), 40_000);
    }
}
```

Запуск:

```bash
cargo test -p sync
```

Этот тест проверяет не только `try_lock`, но и `Acquire`/`Release`: если ordering неверный, счётчик с большой вероятностью разойдётся.

## Что именно проверять у `try_lock`

| Проверка | Ожидание |
|---|---|
| Свободный lock | `try_lock().is_some()` |
| Lock уже удерживается | `try_lock().is_none()` |
| После `drop(guard)` | снова `Some` |
| Чтение/запись данных | значение сохраняется |
| 1000 циклов lock/drop | нет зависания |
| `SpinLock<u32>: Send + Sync` | компилируется |
| 4 потока × 10 000 инкрементов | ровно 40 000 (host) |

## Подводные камни

> [!warning]
> - `try_lock()` возвращает `Option`, а не ошибку: `None` — нормальный результат, не паника.
> - В selftest нельзя дважды вызвать `lock()` без `drop`: будет deadlock.
> - `#[cfg(test)]` в бинарнике для `aarch64-unknown-none` не даст `cargo test` без test harness.
> - Host-тесты возможны только для чистой логики без asm/board/linker.
> - `std::thread`-тест требует `#![cfg_attr(not(test), no_std)]` и отсутствия arch-зависимостей.
> - Для настоящей проверки IRQ-взаимодействия нужен тест в QEMU с прерыванием, а не host-потоки.

## Связанные заметки
- [[spinlock]], [[stage4_sync]], [[atomic_counter]], [[memory_barriers]]
