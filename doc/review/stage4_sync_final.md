---
tags: [review, этап-4, spinlock, selftest, commit]
aliases: [sync final review]
stage: 4
---

# Ревью этапа 4 после реализации

> [!abstract] Суть
> `SpinLock`, UART и heap переведены на блокировки; `cargo build`, `cargo clippy`, `cargo fmt` чистые; selftest проходит, heap-тест тоже. Остался один диагностический баг: `selftest: OK` печатается до `heap: ok`. После исправления и обновления README этап 4 можно закрыть.

## Проверено

| Проверка | Результат |
|---|---|
| `cargo build` | чисто |
| `cargo clippy --bin ... -- -W clippy::all` | чисто |
| `cargo clippy --features selftest --bin ...` | чисто |
| `cargo fmt -- --check` | чисто |
| `cargo run --features selftest` | `try_lock/data/Send+Sync/heap: ok`, затем `ticks` |
| `git status` | чисто |

## Находка

1. **`selftest: OK` печатается слишком рано.**
   - `src/selftest/test_sync.rs` печатает `selftest: start` и `selftest: OK`.
   - `src/selftest/mod.rs` после `test_sync::run()` вызывает `test_heap::run()`.
   - Итог: `selftest: OK` появляется **до** `heap: ok`, то есть «OK» не покрывает heap-тест.
   - Фикс: перенести `start`/`OK` в `mod.rs`:
     ```rust
     pub fn run() {
         crate::println!("selftest: start");
         test_sync::run();
         test_heap::run();
         crate::println!("selftest: OK");
     }
     ```
     и убрать эти два `println!` из `test_sync::run()`.

## Что сделано

- `sync.rs`: `SpinLock`, `SpinLockGuard`, `try_lock`, `#[must_use]` на guard.
- `uart.rs`: `Uart { inner: SpinLock<Pl011> }`, `with`/`try_with`, `try_print_args`.
- `heap.rs`: `Bump { state: SpinLock<BumpState> }`, `GlobalAlloc for Bump`, `alloc_inner`.
- `main.rs`: panic handler печатает через `try_print_args`.
- `irq.rs`: `unexpected` печатает через `try_print_args`.
- selftest: sync + heap.

## Что дальше

1. Исправить порядок `selftest: start/OK`.
2. Обновить README: этап 4 `[x]`, `sync.rs` больше не «план», UART/heap под `SpinLock`, `selftest/` в дереве.
3. Закоммитить `fix: selftest order` и `docs: stage 4`.
4. Переходить к этапу 5 — планировщик.

## Связанные заметки
- [[stage4_sync_precommit]], [[spinlock]], [[no_std_testing]], [[stage4_sync]]
