---
tags: [review, этап-5, шаг-0.3, idle, в-работе]
aliases: [0.3 review, idle review]
stage: 5
---

# Ревью 0.3: что упущено

> [!abstract] Суть
> Функционально шаг работает: default-сборка печатает тики ~4 Гц из idle, `schedtest` доходит до вердикта и затем отдаёт CPU idle, `selftest` проходит. Но clippy красный в трёх конфигурациях из четырёх: `unreachable statement` из-за `spawn_tasks() -> !` и `dead_code` у стека idle. Плюс исчезла строка `schedtest: start`, а негативная проверка «idle только при пустой очереди» не сделана.

## Проверено

| Проверка | Результат |
|---|---|
| `cargo run` | ✅ `ticks` 30 строк за 8 с (~3.75 Гц), значения растут → `wfi` спит |
| `cargo run --features schedtest` | ✅ `switch ok`, `canary ok`, затем `ticks` из idle (путь `exit → idle`) |
| `cargo run --features selftest` | ✅ все группы policy, затем `ticks` из idle |
| `cargo fmt --check` | ✅ |
| clippy `default` | ❌ 1 ошибка (`Stack.0`) |
| clippy `selftest` | ❌ 1 ошибка (`Stack.0`) |
| clippy `schedtest` | ❌ 2 ошибки (`unreachable statement`, `Stack.0`) |
| clippy `selftest,schedtest` | ❌ 2 ошибки (то же) |

## Блокеры

### 1. `spawn_tasks() -> !` дублирует `run()`

```rust
// schedtest/mod.rs
pub fn spawn_tasks() -> ! {
    // … spawn × 3 …
    scheduler::run()          // ← лишнее
}

// main.rs
schedtest::spawn_tasks();     // diverges
scheduler::run();             // unreachable
```

Компилятор предупреждает `unreachable statement` (main.rs:73), а `-D warnings` делает это ошибкой. По смыслу `spawn_tasks` должна только спавнить:

```rust
pub fn spawn_tasks() {
    // spawn × 3
}
```

и `scheduler::run()` остаётся один — в `main`.

### 2. `Stack.0` в `idle.rs` never read

```rust
#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);
```

Поле читается только через `addr_of_mut!` → `dead_code`. Два варианта:

- **A (рекомендуется):** отдать idle загрузочный стек и убрать свою статику:

  ```rust
  // main.rs: stack_top — символ из linker.ld, уже используется для печати
  let boot_stack_top = &raw const stack_top as usize;
  scheduler::spawn(idle::task, "idle", scheduler::Prio::IDLE, boot_stack_top).expect("idle slot");
  ```

  Кадры `main` после `run()` больше не нужны, так что idle может занять эти 64 КиБ. Экономит 8 КиБ `.bss`, убирает статику и предупреждение.

- **B:** оставить отдельный стек и поставить `#[allow(dead_code)]` (как у `schedtest::Stack`).

## Прочие находки

| # | Находка | Что делать |
|---|---|---|
| 1 | Строка `schedtest: start` исчезла из вывода | вернуть в начало `spawn_tasks()` или в cfg-блок `main` |
| 2 | 64 КиБ загрузочного стека после `run()` не используются | вариант A выше закрывает и это |
| 3 | Модульный doc `scheduler.rs` всё ещё «только policy (шаг 0.1)» | «policy (0.1) и cooperative switch (0.2)» |
| 4 | Нет негативной проверки 0.3 | временная задача, которая никогда не выходит и делает `yield_now`: тики должны **перестать** печататься (idle не выбирается, пока Ready не пуст) |
| 5 | У стека idle нет канарейки | только если остаётся вариант B |
| 6 | `schedtest` печатает `FAIL timeout` и уходит в idle | ок, но убедитесь, что `grep FAIL` отрабатывает до появления тиков |

## Негативная проверка 0.3

Это не формальность: без неё «тики идут» не доказывает, что idle выбирается только при пустой очереди.

```rust
// временно в schedtest: задача не завершается
extern "C" fn hog() -> ! {
    loop { scheduler::yield_now(); }
}
```

Ожидание: `sched: switch ok` есть, `ticks` **нет** (idle голодает по приоритету). Убрать задачу — тики вернулись.

## Проверка после правок

```bash
cargo run                                   # тики ~4/с из idle
cargo run --features schedtest              # schedtest: start, switch/canary ok, затем тики
cargo run --features selftest               # policy, затем тики
cargo clippy -- -D warnings
cargo clippy --features selftest -- -D warnings
cargo clippy --features schedtest -- -D warnings
cargo clippy --features "selftest schedtest" -- -D warnings
cargo fmt --check
```

## Текущее состояние (после первых правок)

| Что | Статус |
|---|---|
| `spawn_tasks()` больше не вызывает `run()` | ✅ |
| Но сигнатура осталась `-> !` | ❌ `E0308` + `unreachable statement` в `schedtest`-сборках |
| `hog()` добавлен | ⚠️ нигде не вызывается → `dead_code`; стека для него нет |
| `idle::Stack.0` | ❌ `dead_code` (поле читается только через указатель) |
| `schedtest: start` | ❌ пропала из вывода |
| doc модуля `scheduler.rs` | ❌ всё ещё «только policy (шаг 0.1)» |

Порядок такой: сначала сигнатура (иначе `schedtest` не собирается), затем негативный тест с `hog`, потом решение по стеку idle, затем косметика.

## Итог повторной проверки (2026-11-06)

| Проверка | Результат |
|---|---|
| clippy: default / selftest / schedtest / оба | ✅ 4/4 чисто |
| `cargo fmt --check` | ✅ |
| `cargo run` | ✅ 31 строка `ticks` за 8 с (~3.9 Гц), значения растут → `wfi` спит |
| `cargo run --features schedtest` | ✅ `switch ok`, `canary ok`, затем 47 строк `ticks` (путь `exit → idle`) |
| `cargo run --features selftest` | ✅ все группы policy, затем тики |
| Негативный тест (`hog` в копии) | ✅ вердикт есть, `ticks = 0` — idle не выбирается |

Закрыто: `spawn_tasks() -> ()`, idle на загрузочном стеке (статики и `Stack.0` больше нет), `field_reassign`, `timer::ticks` без allow, `hog` убран из `src/`.

Осталось (не блокеры):

- строка `schedtest: start` так и не вернулась в вывод;
- doc-комментарий модуля `scheduler.rs` обрезан (нет первой строки) и говорит «Переключение контекста — 0.2» как о будущем;
- негативный тест живёт только в этом ревью — в репозитории его нет, регрессию автоматически не поймать (вариант: фича `hog`);
- изменения не закоммичены.

## Связанные заметки
- [[stage5_0_3_idle]], [[stage5_schedtest]], [[stage5_0_2_precommit]], [[roadmap_5_12]]
