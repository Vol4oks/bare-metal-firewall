---
tags: [review, этап-5, шаг-0.4, события, в-работе]
aliases: [0.4 review]
stage: 5
---

# Ревью 0.4: что осталось

> [!abstract] Суть
> Базовая цепочка работает: таймер → реестр `WAKERS` → `PENDING`/`READY` → drain в `yield_now` → тикер печатает `ticks = N (+1)` четыре раза в секунду. До закрытия шага осталось: clippy упадёт на стеке тикера (нет `allow`), опечатка в `schedtest: star`, не прогнаны негативные сценарии и совместный запуск с `schedtest`.

## Что проверено

| Проверка | Результат |
|---|---|
| `yield_now` вызывает `drain_irq` | ✅ (строка 31) |
| `TICKS.fetch_add` с `Ordering::Release`, `ticks()` с `Acquire` | ✅ |
| `wait_current` → `switch_between` → `sched_switch` | ✅ |
| `wake_from_isr` порядок `PENDING` → `READY` | ✅ |
| реестр `WAKERS` + `set_waker`/`wake_from_event` | ✅ |
| прогон: `ticks = 1..22 (+1)` | ✅ |

## Блокер

`ticker.rs`:

```rust
#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);
```

Поле читается только через адрес в `stack_top()`, поэтому `rustc` выдаст `field 0 is never read`, а `clippy -D warnings` сделает это ошибкой. Ровно та же история была у `idle` и `schedtest`.

Два варианта:

- `#[allow(dead_code)]` с комментарием — как в `schedtest`;
- без структуры: `static mut STACK: [u128; STACK_SIZE / 16] = [0; STACK_SIZE / 16];` (выравнивание 16, поля нет).

## Опечатка

`schedtest/mod.rs`:

```rust
crate::println!("schedtest: star");
```

→ `"schedtest: start"`. Строка контрактная: по ней ищут начало теста.

## Не прогнано

| Что | Ожидание |
|---|---|
| `cargo run --features schedtest` | `schedtest: start`, `switch ok`, `canary ok`, затем `ticks = N (+1)` |
| `cargo run --features selftest` | policy-тесты, затем `ticks = N (+1)` |
| clippy 4 конфигурации, `cargo fmt --check` | чисто |
| негатив: маска | тикер ждёт `NET_RX` → тиков нет |
| негатив: счётчик | задержка тикера → `(+2)`/`(+3)` |
| негатив: без waker | `set_waker` не вызван → тиков нет |

## Негативные проверки (временно, потом откатить)

**Маска.** В `ticker::task`:

```rust
scheduler::wait_current(events::NET_RX); // вместо TICK
```

Ожидание: после `VBAR` тишина — таймер шлёт `TICK`, маска не совпадает. Если тики идут — `wake` игнорирует `wait_mask`.

**Счётчик.** Перед ожиданием:

```rust
let target = timer::ticks() + 2;
while timer::ticks() < target {
    core::hint::spin_loop();
}
```

Ожидание: первый вывод после этого — `(+2)` (или больше), затем снова `(+1)`.

**Без waker.** Закомментировать `scheduler::set_waker(events::TICK, tick);`.
Ожидание: тиков нет, хотя `TICKS` растёт (проверить нечем, но отсутствие вывода достаточно).

## Мелочи

- `#![allow(dead_code)]` в `scheduler.rs` с `TODO(0.4)`: после 0.4 мёртвыми остаются `Prio::new/get`, `Task.name`, `assert_invariants` — обновить комментарий (TODO → этап 6 или 0.5).
- `module`-doc `scheduler.rs` всё ещё без первой строки и говорит «Переключение контекста — 0.2».
- `yield_now` держит `&mut SCHED` через `sched_switch`; безопасно (после возврата не используется), но `wait_current` уже сделан аккуратнее — можно выровнять.
- Политест на drain в `selftest` пока не добавлен: `wake_from_isr` + `drain_irq` → `Ready`; `set_waker` + `wake_from_event` → `PENDING` нужного id.

## Итог

Всё из списка закрыто:

- `ticker` использует один способ описания стека (`struct Stack` + `size_of`), без второго массива;
- `schedtest: start` на месте, в `test_sched` вернулась строка `sched: start`;
- негативные сценарии пройдены: неверная маска → тиков нет; сдвиг счётчика → `(+2)`; без `set_waker` → тиков нет;
- `just ci` (fmt, clippy × 4, сборки) и `just smoke` зелёные.

Осталось только удалить закомментированную строку негативного теста в `ticker.rs` и починить doc-комментарий модуля `scheduler.rs` — это подготовка к коммиту, а не блокер.

## Связанные заметки
- [[stage5_0_4_events]], [[stage5_0_3_idle]], [[stage5_schedtest]], [[fp_simd_in_kernel]]
