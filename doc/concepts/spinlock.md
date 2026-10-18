---
tags: [синхронизация, spinlock, atomics, этап-4]
aliases: [SpinLock, spin lock]
stage: 4
---

# SpinLock: взаимное исключение без сна

> [!abstract] Суть
> SpinLock — это `AtomicBool`-флаг + `UnsafeCell<T>`. Захват: `compare_exchange(false, true, Acquire)`. Освобождение: `store(false, Release)`. Подходит для коротких критических секций в bare-metal, где нельзя усыпить поток.

## Реализация

```rust
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

// &SpinLock можно передавать в несколько контекстов; lock даёт
// эксклюзивный &mut T. T должен быть Send: его «передают» между контекстами.
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

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> Deref for SpinLockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinLockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}
```

## Почему такие Ordering

| Операция | Ordering | Почему |
|---|---|---|
| успешный `compare_exchange` | `Acquire` | всё, что было до освобождения другим контекстом, видно после захвата |
| неуспешный `compare_exchange` | `Relaxed` | мы только повторяем попытку, данные не читаем |
| `store(false)` | `Release` | записи внутри критической секции видны следующему владельцу |

`Relaxed` на захвате сломал бы видимость данных, `SeqCst` — избыточен.

## IRQ-ловушка

```text
main: lock(UART)
IRQ:  timer -> обработчик печатает -> lock(UART) -> вечный spin
```

На одном ядре это deadlock: прерванный контекст не отпустит lock, пока обработчик не вернётся.

Варианты:

- **ISR не берёт lock** — текущая политика: IRQ-обработчик не печатает и не аллоцирует.
- **`try_lock` в panic/ISR** — если lock занят, пропустить вывод, а не зависнуть.
- **`IrqSafeSpinLock`** — маскировать IRQ на время удержания lock.

```rust
pub struct IrqSafeSpinLock<T> {
    inner: SpinLock<T>,
}

pub struct IrqSafeGuard<'a, T> {
    guard: Option<SpinLockGuard<'a, T>>,
    daif: u64,
}

impl<T> IrqSafeSpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self { inner: SpinLock::new(data) }
    }

    pub fn lock(&self) -> IrqSafeGuard<'_, T> {
        let daif = unsafe { disable_irq() };
        let guard = self.inner.lock();
        IrqSafeGuard { guard: Some(guard), daif }
    }
}

impl<T> Drop for IrqSafeGuard<'_, T> {
    fn drop(&mut self) {
        // Сначала отпускаем lock, потом разрешаем IRQ.
        self.guard.take();
        unsafe { restore_irq(self.daif) };
    }
}
```

`disable_irq`/`restore_irq`:

```rust
unsafe fn disable_irq() -> u64 {
    let daif: u64;
    unsafe {
        core::arch::asm!("mrs {}, daif", out(reg) daif);
        core::arch::asm!("msr daifset, #2", "isb");
    }
    daif
}

unsafe fn restore_irq(daif: u64) {
    unsafe {
        core::arch::asm!("msr daif, {}", in(reg) daif, "isb");
    }
}
```

`#2` — бит `I` (IRQ). `msr daif, x` восстанавливает все четыре маски, включая вложенные случаи.

> [!warning] Порядок в Drop
> Нельзя сначала разрешать IRQ, а потом отпускать lock: отложенное прерывание может выстрелить между этими действиями и снова встать на тот же lock. Поэтому `guard.take()` — до `restore_irq`.

## Подводные камни

> [!warning]
> - Долгая критическая секция (UART, ожидание железа) → другие контексты крутятся вхолостую.
> - `Relaxed` для lock → гонки на данных, хотя сам флаг атомарен.
> - Забыли `Drop`/guard → lock останется занятым навсегда.
> - `try_lock` в `GlobalAlloc`/панике: если lock занят, нельзя ждать — нужен явный fallback.
> - `SpinLock` не защищает от IRQ на том же ядре; для этого нужен `IrqSafeSpinLock` или запрет lock в ISR.
> - `critical_section` (маска IRQ) не заменяет SpinLock для SMP: он не даёт взаимного исключения между ядрами.

## Связанные заметки
- [[atomic_counter]], [[memory_barriers]], [[stage4_sync]], [[0002_global_uart_handle]], [[multicore_path]]
