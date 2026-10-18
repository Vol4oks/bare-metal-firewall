---
tags: [этап-4, синхронизация, spinlock, uart, heap]
aliases: [stage4, synchronization]
stage: 4
---

# Этап 4: синхронизация

> [!abstract] Суть
> После включения IRQ старые инварианты «IRQ замаскированы» перестали быть правдой. Этап 4 заменяет обещания `unsafe impl Sync` настоящими блокировками: `SpinLock` для UART и heap, `try_lock` в панике, при необходимости — `IrqSafeSpinLock`.

## DoD — этап закрыт, когда

- `UART` и `HEAP` больше не держатся на голом `UnsafeCell` + `unsafe impl Sync` без блокировки;
- `cargo run` печатает растущие `ticks`;
- `panic!` не зависает, если паника случилась во время печати;
- `cargo clippy --bin bare-metal-firewall` без ошибок;
- инварианты и ограничения записаны в doc.

## Шаги

1. `sync.rs`: `SpinLock<T>`, `SpinLockGuard`, `try_lock`.
2. Самотест: `let g = LOCK.lock(); assert!(LOCK.try_lock().is_none()); drop(g);`.
3. `uart.rs`: `static UART: SpinLock<Pl011>`; убрать обёртку `Uart` с `UnsafeCell`/`unsafe impl Sync`.
4. `heap.rs`: `static HEAP: SpinLock<Bump>`; поля `Bump` становятся обычными (`ptr: usize`, `end: usize`), эксклюзивность даёт lock.
5. Паника: печатать через `UART.try_lock()`; если занято — пропустить вывод.
6. Если ISR начнёт печатать/аллоцировать — `IrqSafeSpinLock` или запрет lock в ISR.

## UART

```rust
use crate::board;
use crate::sync::SpinLock;

pub static UART: SpinLock<Pl011> = SpinLock::new(Pl011::new(board::UART_BASE));

pub fn print_args(args: core::fmt::Arguments<'_>) {
    let mut uart = UART.lock();
    let _ = core::fmt::write(&mut *uart, args);
}
```

Паника:

```rust
if let Some(mut uart) = UART.try_lock() {
    let _ = core::fmt::write(&mut *uart, format_args!("PANIC: {}\n", info.message()));
}
// иначе — молча остановиться, но не зависнуть
```

`Pl011::write_byte` уже принимает `&mut self`; guard даёт `&mut Pl011` через `DerefMut`.

## Heap

```rust
struct Bump {
    ptr: usize,
    end: usize,
}

impl Bump {
    const fn new() -> Self {
        Self { ptr: 0, end: 0 }
    }

    fn init(&mut self, start: usize, end: usize) {
        self.ptr = start;
        self.end = end;
    }

    fn alloc(&mut self, layout: core::alloc::Layout) -> *mut u8 {
        let Some(aligned) = align_up(self.ptr, layout.align()) else {
            return core::ptr::null_mut();
        };
        let Some(new_p) = aligned.checked_add(layout.size()) else {
            return core::ptr::null_mut();
        };
        if new_p > self.end {
            return core::ptr::null_mut();
        }
        self.ptr = new_p;
        aligned as *mut u8
    }
}

static HEAP: SpinLock<Bump> = SpinLock::new(Bump::new());

unsafe impl GlobalAlloc for SpinLock<Bump> {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut bump = self.lock();
        unsafe { bump.alloc(layout) }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}
```

`alloc` получает `&self`, но lock даёт `&mut Bump`; внутри `Bump` больше не нужен `UnsafeCell`. `dealloc` пустой — память не переиспользуется.

## Критическая секция vs SpinLock

| Механизм | Защищает | Не защищает |
|---|---|---|
| `critical_section` / маска IRQ | от IRQ на **этом** ядре | от других ядер |
| `SpinLock` | от других контекстов/ядер | от IRQ-deadlock на том же ядре |
| `IrqSafeSpinLock` | и от IRQ, и от других ядер | от неправильного lock ordering |

Для текущего этапа (одно ядро, ISR не печатает и не аллоцирует) достаточно `SpinLock` + `try_lock` в панике. `IrqSafeSpinLock` понадобится, когда ISR начнёт трогать UART/heap.

## Тесты

- `cargo run`: `ticks` растут.
- Печать из `main` при работающем таймере: нет зависаний.
- `try_lock`-самотест: повторный захват возвращает `None`.
- `Box::new(...)` после замены heap работает.
- `panic!` во время печати: panic handler не зависает (печатает или молча останавливается).

## Подводные камни

> [!warning]
> - Держать lock во время долгого UART-вывода — блокировать другие контексты; критическая секция должна быть минимальной.
> - В `panic_handler` нельзя `lock()`: паника могла случиться под этим же lock. Только `try_lock`.
> - `GlobalAlloc` не умеет «подождать»: если lock занят, `alloc` зависнет. Не аллоцировать из ISR/паники.
> - `IrqSafeGuard` должен отпускать lock **до** разрешения IRQ.
> - Не смешивать порядок захвата блокировок: если когда-нибудь будет две, зафиксировать порядок.

## Связанные заметки
- [[spinlock]], [[atomic_counter]], [[memory_barriers]], [[0002_global_uart_handle]], [[global_alloc]], [[multicore_path]]
