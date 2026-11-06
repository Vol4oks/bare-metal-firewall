---
tags: [review, этап-5, schedtest, шаг-0.2, незавершено]
aliases: [schedtest review]
stage: 5
---

# Ревью `schedtest` (0.2, в работе)

> [!abstract] Суть
> Структура стенда соответствует плану: отдельный feature-модуль, три стека, маркеры на стеке, атомарные счётчики, монитор того же приоритета. Но сборка падает с 23 ошибками: слой механизма 0.2 (`sched_switch`, `SCHED`, `yield_now`/`exit`/`run`, заполнение `ctx`) ещё не написан, плюс 5 локальных недоделок в самом модуле. Отдельно: `default = ["selftest", "schedtest"]` меняет поведение «обычного» `cargo run`.

## Повторная проверка (после правок)

Сборка: **11 ошибок**, все — `cannot find function {spawn, yield_now, exit, run} in module scheduler`. `cargo fmt --check` чист.

Закрыто из списка выше:

- `Ordering` импортирован, `Prio` используется через путь `scheduler::Prio::CP`;
- `task_b` и `canary_ok` добавлены;
- у монитора появился лимит ожидания (`spins > 10_000` → `sched: FAIL timeout`);
- форматирование чистое.

Осталось — это уже не тесты, а механизм 0.2:

- `static mut SCHED` и free-обёртки `spawn`/`yield_now`/`exit`/`run`;
- заполнение `ctx.sp`/`ctx.x30` в `Scheduler::spawn`;
- `src/sched_switch.s` + `global_asm!` + `unsafe extern "C"`.

Открытый вопрос: `default = ["schedtest"]`. Удобно для разработки, но ломает документированные команды (`cargo run --features selftest` включает оба режима) и DoD «обычный запуск не изменился». Перед закрытием 0.2 вернуть `default = []`.

## Что сделано правильно

- Модуль отделён от `selftest` и закрыт feature'ом — граница policy/mechanism соблюдена ([[stage5_0_2_switch]]).
- Три стека, а не два: монитор — тоже задача.
- `Stack([0; N])` → `.bss`, образ не растёт; канарейка пишется `write_volatile` в основание.
- Маркер `0xA5`/`0x5A` на стеке + `black_box` — ловит перепутанные стеки/регистры, а не только переполнение.
- Счётчики на `AtomicU32`/`AtomicBool` с `SeqCst` — тест не смешивается с проверкой `SpinLock`.
- Монитор с `Prio::CP`, а не `DP` — при строгом приоритете DP-монитор заморил бы рабочие задачи.
- `main`: `#[cfg(feature)]` + `#[cfg(not(...))]` для цикла тиков — правильный паттерн, без `unreachable_code`.
- Ветка FAIL печатает все три диагностики (`counts`, `errors`, `canary`), а не просто «ошибка».

## Блокеры: не компилируется (23 ошибки)

| # | Ошибка | Причина | Что нужно |
|---|---|---|---|
| 1 | `cannot find type Ordering` | не импортирован | `use core::sync::atomic::Ordering;` |
| 2 | `cannot find type Prio` | не импортирован | `use crate::scheduler::Prio;` |
| 3 | `cannot find function spawn in module scheduler` | `spawn` — метод `Scheduler`, свободной функции нет | добавить free-обёртку (см. ниже) |
| 4 | `cannot find function yield_now` | 0.2 не реализован | asm + `yield_now` |
| 5 | `cannot find function exit` | 0.2 не реализован | `exit() -> !` через `sched_start` |
| 6 | `cannot find function run` | 0.2 не реализован | `run() -> !` через `sched_start` |
| 7 | `cannot find value task_b` | функция не написана | зеркало `task_a` с `0x5A`/`B_COUNT`/`B_DONE` |
| 8 | `cannot find function canary_ok` | функция не написана | чтение первого `u64` стека |

Пункты 3–6 — это и есть весь шаг 0.2: в `scheduler.rs` пока нет `global_asm!`, `static mut SCHED`, заполнения `ctx` и asm-обёрток.

## Находки по дизайну

1. **`default = ["selftest", "schedtest"]`.** Теперь «обычный» `cargo run` печатает selftest и уходит в планировщик, а `cargo run --features selftest` включает оба режима. Это ломает контракт из [[stage5_policy_testing]] и DoD «обычный запуск не изменился». Рекомендация: `default = []`, запуск всегда явный. Если оставляете как есть — команды должны быть `--no-default-features`.
2. **Монитор ждёт бесконечно.** Если одна из задач не доходит до `DONE` (баг в переключении), тест не печатает вердикт, а висит до `timeout`. Лучше ограничить ожидание счётчиком итераций и напечатать `sched: FAIL timeout`. Иначе «зависло» и «сломалось» выглядят одинаково.
3. **API свободных функций не определён.** В модуле смешаны два подхода: `scheduler::spawn(...)` (свободная функция) и методы `Scheduler`. Нужен один стиль. Рекомендуется набор обёрток вокруг `static mut SCHED`:
   ```rust
   pub fn spawn(entry: extern "C" fn() -> !, name: &'static str, prio: Prio, stack_top: usize) -> Option<usize>;
   pub fn yield_now();
   pub fn exit() -> !;
   pub fn run() -> !;
   ```
4. **Канарейка ловит только полный износ стека.** Она стоит в основании: если задача уйдёт ниже основания, она затрёт соседнюю статику, а не свою канарейку. Это не отменяет её, но маркер в кадре остаётся главной проверкой «контекст цел».
5. **`#![allow(dead_code)]` в `scheduler.rs` пока нужен**: `assert_invariants` используется только в `selftest`, а в сборке с одним `schedtest` будет мёртвым. Снимать на 0.3 (см. [[stage5_0_2_switch]]).

## Порядок закрытия 0.2

1. Импорты, `task_b`, `canary_ok` — модуль должен собраться (пусть и с несуществующим API).
2. `default = []` в `Cargo.toml`.
3. `ctx.sp`/`ctx.x30` в `spawn`.
4. `src/sched_switch.s` + `global_asm!` + `unsafe extern "C"`.
5. `static mut SCHED` + free-обёртки (`spawn`/`yield_now`/`exit`/`run`).
6. CP1: две бесконечные задачи печатают `A`/`B` → глазами.
7. CP2–CP3: монитор и канарейки → `sched: switch ok` / `sched: canary ok`.
8. CP4: намеренная поломка (`x30 = 0`, без канарейки, стек 256 Б) → тест обязан упасть.

## DoD 0.2

- `cargo run --features schedtest` → `sched: switch ok`, `sched: canary ok`;
- `cargo run` → прежние тики (при `default = []`);
- `cargo clippy --features schedtest -- -D warnings` и `cargo fmt --check` чисты;
- негативная проверка CP4 выполнена и откатана.

## Прогон в QEMU (после реализации механизма)

- `cargo build` (debug и release) — собирается и линкуется;
- `cargo run` → `schedtest: start`, затем
  `PANIC: SYNC: EC=0x7 ISS=0x1e00000 FAR=0x0 ELR=0x42001040`;
- по `nm`, `ELR` = `task_a + 8` — значит `sched_start` **сработал**: управление дошло до задачи на её собственном стеке;
- причина паники — FP/SIMD trap: `CPACR_EL1.FPEN = 0`, а инициализация `[0xA5; 64]` векторизована в NEON. Подробно — [[fp_simd_in_kernel]].

### Находки прогона

| # | Находка | Серьёзность |
|---|---|---|
| 1 | FP/SIMD trap блокирует прогон (`EC=0x7`) | блокер |
| 2 | `sched_switch`/`sched_start` — локальные символы (`nm` печатает `t`), нет `.section`/`.global` | высокий риск при выносе asm в другой крейт |
| 3 | `clippy -D warnings`: 8 ошибок — `dead_code` из-за `default=["schedtest"]` и `field_reassign_with_default` | блокер для DoD |
| 4 | `Stack.0` never read (доступ только через сырые указатели) | нужен `#[allow(dead_code)]` |
| 5 | Задачи ничего не печатают — первый сигнал только вердикт монитора | удобство отладки |
| 6 | `default = ["schedtest"]` | вернуть `[]` перед закрытием |

### Что делать

1. Включить FPEN в `_start` и добавить `q8`–`q15` в `TaskContext` + asm (или отказаться от SIMD — не рекомендуется, см. [[fp_simd_in_kernel]]).
2. Добавить `.section .text` и `.global sched_switch` / `.global sched_start`.
3. `TaskContext { sp, x30, ..Default::default() }` вместо двух присваиваний.
4. Вернуть `#![allow(dead_code)]` с `TODO(0.3)` и `#[allow(dead_code)]` на `Stack`.
5. Одноразовый `println!` в начале каждой задачи — для CP1.
6. Перезапуск: ожидаем `sched: switch ok` и `sched: canary ok`.

## Результат после фикса FPEN (проверено)

Причина «пустого» лога — не планировщик: в `_start` блок FPEN использовал `x0`, а тот был курсором обнуления `.bss`. Цикл начинал писать с адреса `0x300000`; abort до установки `VBAR` → ни одного байта в UART.

После замены `x0` → `x2` (прогон в копии проекта):

```text
Hello from QEMU virt!
...
schedtest: start
sched: switch ok
sched: canary ok
```

Остаётся:

- **`sched_start` не восстанавливает `q8`–`q15`** (в `sched_switch` они есть). Через `sched_start` идёт `exit()`, поэтому возобновлённая задача потеряет callee-saved SIMD. В этом тесте не проявилось (маркер живёт на стеке), но это тихая порча — [[fp_simd_in_kernel]].
- `.global`/`.section .text` в asm — нужно для будущего выноса в отдельный крейт.
- clippy `dead_code` (7 warnings) — allow до 0.3 или запуск с обеими фичами.
- `default = ["schedtest"]` — решить перед закрытием.

## Стенд слеп к отсутствию переключений (найдено экспериментом)

В копии проекта закомментированы **все** `scheduler::yield_now()` — прогон всё равно печатает:

```text
sched: switch ok
sched: canary ok
```

Причина: без переключений `task_a` доделывает 10 итераций и вызывает `exit()`; тот переводит на `task_b` через `sched_start`; затем монитор видит `10/10` и ставит вердикт. Счётчики такую поломку не ловят.

Лечение — проверка чередования через `LAST`-атомик, см. [[stage5_schedtest]]. После неё тот же эксперимент обязан давать `sched: FAIL … errors>0`.

## Связанные заметки
- [[stage5_schedtest]], [[stage5_0_2_switch]], [[stage5_policy_testing]], [[stage5_tdd]]
