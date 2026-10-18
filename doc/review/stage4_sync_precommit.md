---
tags: [review, этап-4, spinlock, selftest, commit]
aliases: [sync precommit, spinlock review]
stage: 4
---

# Ревью SpinLock и selftest перед коммитом

> [!abstract] Суть
> `SpinLock` реализован корректно, selftest проходит, `cargo fmt` чист, `cargo clippy --features selftest` чист. Для коммита «sync + tests» нужно убрать 3 `dead_code` warning'а в обычной сборке. Для закрытия этапа 4 осталось перевести UART и heap на `SpinLock`.

## Проверено

| Проверка | Результат |
|---|---|
| `cargo build` | 3 warnings: `SpinLock`/`new`/`lock`/`try_lock`/`SpinLockGuard` не используются |
| `cargo clippy --features selftest` | чисто |
| `cargo fmt -- --check` | чисто |
| `cargo run --features selftest` | `selftest: OK`, затем `ticks` растут |
| UART/heap | всё ещё `UnsafeCell` + `unsafe impl Sync` |

Вывод selftest:

```text
selftest: start
  try_lock: ok
  data: ok
  Send + Sync: ok
selftest: OK
```

## Находки

### Блокер для обычной сборки

1. **`mod sync;` объявлен безусловно.** В обычной сборке `SpinLock` используется только в `selftest`, поэтому появляются `dead_code` warnings.
   - Вариант A (чище для отдельного коммита): `#[cfg(feature = "selftest")] mod sync;`, а после перевода UART/heap убрать `cfg`.
   - Вариант B: `#[allow(dead_code)]` на `mod sync;` с TODO «будет использован в UART/heap».

### Этап 4 ещё не закрыт

2. **UART и heap не переведены на `SpinLock`.**
   - `uart.rs` всё ещё `Uart { UnsafeCell<Pl011> }` + `unsafe impl Sync`.
   - `heap.rs` всё ещё `Bump` с `UnsafeCell` + `unsafe impl Sync`.
   - То есть `SpinLock` пока существует сам по себе и проверяется только selftest'ом.
   - Следующий шаг: UART под `SpinLock`, затем heap; после этого `mod sync;` становится безусловным.

### Качество API

3. **Добавить `#[must_use]` на `SpinLockGuard`.**
   ```rust
   #[must_use = "if unused, the lock is released immediately"]
   pub struct SpinLockGuard<'a, T> { ... }
   ```
   Иначе случайный `lock.lock();` без сохранения guard'а мгновенно отпустит lock, и это легко пропустить.

### Ограничение тестов

4. **Selftest проверяет состояние, но не конкуренцию.**
   - Он ловит: «lock не отпускается», «try_lock врёт», «данные не сохраняются», «нет Send/Sync».
   - Он **не** проверяет mutual exclusion и `Acquire`/`Release` под реальной конкуренцией.
   - Для этого — host-тесты `sync`-крейта с `std::thread` (см. [[no_std_testing]]) или будущий SMP/IRQ-тест.
   - Сейчас это допустимо: одно ядро, ISR не трогает UART/heap.

## Что хорошо

- `compare_exchange(false, true, Acquire, Relaxed)` — корректно.
- `store(false, Release)` в `Drop` — корректно.
- `unsafe impl<T: Send> Sync` — правильный bound.
- `Deref`/`DerefMut` дают доступ только через guard.
- Selftest покрывает базовые инварианты и compile-time traits.

## Рекомендуемые коммиты

```text
1. feat: SpinLock + selftest (Cargo.toml, src/sync.rs, src/selftest/, src/main.rs)
2. docs: no_std testing (doc/concepts/no_std_testing.md и ссылки)
3. позже: refactor: UART/heap под SpinLock
```

Перед первым коммитом — убрать `dead_code` warnings (cfg или allow).

## Связанные заметки
- [[spinlock]], [[no_std_testing]], [[stage4_sync]], [[atomic_counter]]
