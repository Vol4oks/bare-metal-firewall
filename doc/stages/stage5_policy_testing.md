---
tags: [этап-5, планировщик, тестирование, policy, selftest, шаг-0.1]
aliases: [policy testing, 0.1 tests]
stage: 5
---

# Тестирование шага 0.1

> [!abstract] Суть
> Policy-логику проверяем на четырёх уровнях: compile-time раскладка (`offset_of!`), сценарные кейсы, инварианты после каждой операции и детерминированный randomized-прогон. Сейчас всё исполняется **selftest'ом в QEMU** — проект ещё один бинарник, host-тестов физически нет; после миграции в workspace те же тела переезжают в `#[cfg(test)]` крейта `fw-sched` и гоняются `cargo test -p fw-sched` на хосте.

## Где и когда прогоняются тесты

| Момент | Где | Команда | Почему так |
|---|---|---|---|
| шаг 0.1 | QEMU, feature `selftest` | `cargo run --features selftest` | бинарник `no_std`/`no_main` + `aarch64-unknown-none`; test harness недоступен ([[no_std_testing]]) |
| после 1.3.4 | host | `cargo test -p fw-sched` / `cargo fwt` | policy без asm, MMIO и линковки |

Почему нельзя на хосте сейчас: `cargo test` для бинарника требует `test`-крейт и host-цель, а `start.s`, `irq_vectors.s`, `#![no_main]` и `linker.ld` на host не собираются. Разделять «политику» и «механизм» в один шаг с миграцией workspace — против выбранного варианта 1A.

## L0. Compile-time проверки

```rust
const _: () = assert!(core::mem::offset_of!(TaskContext, sp) == 96);
const _: () = assert!(core::mem::size_of::<TaskContext>() == 104);
```

- Asm в 0.2 будет ходить по фиксированным смещениям (`[x0, #0]`, `[x0, #96]`). Если кто-то переставит поле, ошибка должна быть на компиляции, а не «регистры поехали в QEMU».
- `#[repr(C)]` обязателен: без него порядок полей не гарантирован.
- Это единственный тест, который «работает» и без запуска кода.

## L1. Матрица состояний — чек-лист

Матрица — не тест-код, а **таблица ожиданий**, по которой пишутся кейсы. Действия, зависящие от текущей задачи, применимы только когда `current == Some(id)` и она `Running`.

| Состояние | `pick_next` | `yield_current` | `exit_current` | `block_current` | `try_wait_current` | `wake` |
|---|---|---|---|---|---|---|
| `Unused` | не выбирается | `None` | `None` | `NotBlocked` | `None` | `false`, `pending` не трогаем |
| `Ready` | выбирается | — | — | `NotBlocked` | `None` | латч, состояние не меняется |
| `Running` | — (себя не выбирает `start`) | `→ Ready`, вернуть next | `→ Exited`, вернуть next | `Blocked{next}` или `NotBlocked` | `Some/None` | латч, остаётся `Running` |
| `Blocked` | не выбирается | — | — | `NotBlocked` | `None` | `→ Ready`, если биты совпали с `wait_mask` |
| `Exited` | не выбирается | — | — | `NotBlocked` | `None` | `false`, `pending` не трогаем |

`block_current` из `Running` разворачивается в две строки:

| `pending & mask` | Результат |
|---|---|
| `== 0` | `Blocked { next }`, `wait_mask = mask` |
| `!= 0` | `NotBlocked`, задача остаётся `Running`, биты в `pending` |

> [!warning] Полную матрицу «состояние × действие» через публичный API не построить
> Чтобы выставить произвольное состояние (например, `Ready`-задачу как `current`), нужен test-only сеттер, который ломает инкапсуляцию. Не стоит: покрываем **достижимые** комбинации через `spawn`/`start`/`block_current`/`exit_current`. Матрица остаётся чек-листом, а не генератором.

## L2. Сценарные кейсы

Каждый кейс — отдельная функция; имя говорящее; после каждого действия — `assert_invariants()`.

| # | Сценарий | Ожидание |
|---|---|---|
| 1 | пустой планировщик | `current == None`; `start`/`yield`/`exit` → `None` |
| 2 | `spawn` × 3 | id `0,1,2`, все `Ready` |
| 3 | `start` при CP(0), DP(1) | выбран DP (меньший `Prio`) |
| 4 | три CP-задачи | `start → 0`, `yield → 1 → 2 → 0` |
| 5 | одна задача, `yield` | возвращает её же |
| 6 | приоритет не вытесняет | CP `Running`, затем `spawn` DP; до `yield` current не меняется |
| 7 | `exit_current` с Ready-задачей | возвращает её id, `Running` ровно один |
| 8 | `exit_current` последней | `None`, `current == None` |
| 9 | `spawn` сверх `MAX_TASKS` | `None`, паники нет |
| 10 | все слоты `Exited`, затем `spawn` | `None` — слоты не переиспользуются |
| 11 | `Blocked`-задача | не выбирается `pick_next`, `wait_mask != 0` |
| 12 | `block_current` без `pending` | `Blocked { next }`, current переключён |
| 13 | `pending` до `block_current` | `NotBlocked`, задача осталась `Running` |
| 14 | `wake` заблокированной своим битом | `Ready`, `wait_mask == 0`, `pick_next` её отдаёт |
| 15 | `wake` чужим битом | остаётся `Blocked`, бит лежит в `pending` |
| 16 | `pending = A\|B`, `try_wait_current(A)` | вернул `A`, `B` остался |
| 17 | `try_wait_current` при пустом `pending` | `None`, состояние не меняется |
| 18 | `wake` для `Running`/`Ready` | `false`, `pending` пополнился, состояние прежнее |
| 19 | `wake` для `Unused`/`Exited` | `false`, `pending == 0` |
| 20 | `Prio::new(x).get()` | round-trip без изменений |

Порядок `1 → 20` — он же порядок реализации: кейсы 1–10 закрывают 0.1.1–0.1.3, 11–19 — 0.1.4.

## L3. Инварианты после каждой операции

`assert_invariants()` вызывается в конце **каждого** кейса и в randomized-прогоне после каждой операции:

- `current == None` ⇒ ни одной `Running`; `current == Some(i)` ⇒ `tasks[i].state == Running`;
- `Running` не больше одной;
- `Ready`/`Running`/`Blocked` ⇒ `entry.is_some()`, `stack_top != 0`, `stack_top % 16 == 0`;
- `Blocked` ⇒ `wait_mask != 0`; `Ready`/`Running` ⇒ `wait_mask == 0`;
- `Unused` ⇒ `entry == None`, `stack_top == 0`, `pending == 0`;
- `Exited` ⇒ `entry == None`, `pending == 0`, `wait_mask == 0`.

Стоимость — один проход по 8 слотам, для тестов бесплатно. В горячем пути не вызывать.

## L4. Randomized-прогон (модель)

Детерминированный xorshift64, фиксированный seed, 10 000 операций. Без аллокаций, без времени — работает одинаково в QEMU и на хосте.

```rust
fn next_rand(s: &mut u64) -> u64 {
    let mut x = *s;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *s = x;
    x
}
```

Набор операций и их доля:

| Операция | Доля | Ограничение |
|---|---|---|
| `spawn` | 10% | пока есть `Unused` |
| `start` (если `current == None`) | 10% | — |
| `yield_current` | 25% | только при `Running` |
| `try_wait_current` | 15% | только при `Running` |
| `block_current` | 15% | только при `Running` |
| `wake(random_id, random_bits)` | 20% | `id` — из уже занятых слотов |
| `exit_current` | 5% | только при `Running` |

Свойства, которые проверяются:

1. Ни одна операция не паникует (кроме ожидаемых `debug_assert` вне прогона).
2. После каждой операции — `assert_invariants()`.
3. `wake` по `Blocked`-задаче с пересечением `pending & wait_mask` **обязан** дать `Ready`.
4. `Exited`-задача никогда не выбирается `pick_next`/`start`.
5. Счётчик «сколько раз задачу будили» не теряется: если `wake` вернул `false` для `Ready`/`Running`, биты должны присутствовать в `pending`.

Seed печатать один раз перед прогоном (`sched: model seed=0x...`): при падении его видно в UART, и прогон воспроизводится бит-в-бит. Это **не доказательство** корректности — это ловля рассинхронизации состояний, которую не покрыли сценарные кейсы.

> [!warning] Randomized-тест не заменяет сценарии
> Он генерирует бессмысленные последовательности (`block` без ожидаемого события и т.п.) и потому проверяет только инварианты. Смысловые ожидания («после `wake` задача получила CPU») задаются в L2.

## L5. Негативные и «паника»-кейсы

- `spawn` с `stack_top % 16 != 0` → `debug_assert` (в release не проверяется: цена в горячем пути).
- `spawn` сверх `MAX_TASKS` → `None`, а не паника.
- `state(id)` для невалидного `id` — сейчас паника по индексации. В тестах используются только валидные id. Если понадобится в отладке/UI — добавить `try_state`.
- В QEMU нельзя проверить «панику без catch_unwind»: `#[should_panic]` появится только в host-тестах после миграции.

## Как устроен прогон в QEMU

1. `Cargo.toml` уже содержит `[features] selftest = []`.
2. `src/selftest/mod.rs`: добавить `mod test_sched;` и вызов между `test_sync::run()` и `test_heap::run()`.
3. Контракт вывода — по строке на группу плюс финальная:

```text
selftest: start
  ...
  sched: states ok
  sched: round-robin ok
  sched: priority ok
  sched: events ok
  sched: model ok
selftest: end
```

- Группа печатает `ok` **после** прохождения всех своих проверок. Отсутствие строки = падение.
- Падение: `assert` → `panic_handler` печатает `PANIC: … at src/…:NN` и уходит в спин. QEMU остаётся жив — это ожидаемо, вывод читается глазами/грепом.

Запуск и автоматизация (QEMU сам не завершается, поэтому `timeout` + греп финальной строки):

```bash
timeout 60 cargo run --features selftest 2>&1 | tee /tmp/selftest.log
grep -q 'sched: model ok' /tmp/selftest.log && echo PASS || echo FAIL
```

После миграции: `cargo fwr --features selftest` (алиас из [[cargo_aliases]]).

> [!question] Задел на будущее
> Чтобы прогон был CI-пригодным, можно добавить semihosting-exit (`-semihosting` + `SYS_EXIT`) и получить код возврата вместо `timeout`. Для 0.1 не нужно — грепа достаточно.

## Что 0.1 принципиально не проверяет

| Не покрыто | Где появится |
|---|---|
| `sched_switch`, сохранение регистров, стеки | 0.2 |
| атомарность `READY`/`PENDING`, гонки с IRQ | 0.4 |
| бюджет DP, дедлайны | 0.4–0.5 |
| вытеснение по кванту | 5.5 (опционально) |
| реальные задержки (QEMU TCG) | только плата, [[latency_measurement]] |

Отдельно: в 0.1 нет ни одного атомика, поэтому **не пишите** «конкурентные» тесты с потоками — они появятся вместе с `PENDING`/`READY` и потребуют рассуждения про `Relaxed`/`Acquire`/`Release` ([[atomic_counter]], [[memory_barriers]]).

## Переезд в `fw-sched` (после 1.3.4)

- Тела кейсов переносятся в `#[cfg(test)] mod tests` без изменения логики: `assert!` уже используется, `println!` убирается.
- Имена тестов сохраняются (`round_robin`, `priority_blocks_cp`, `event_before_block`, …) — по ним потом ищут регрессии.
- Randomized-прогон переносится как есть: xorshift не зависит от `std`.
- Добавляются host-only кейсы: `#[should_panic]` на невыровненный `stack_top`, проверка `size_of`/`offset_of`.
- В selftest остаётся **один** smoke-кейс (`states ok`), чтобы проверять реальную сборку под `aarch64`; остальное живёт на хосте и гоняется `cargo fwt`.
- Дублировать тесты в двух местах не нужно: policy — host, mechanism — QEMU ([[stage5_tdd]]).

## Подводные камни

> [!warning]
> - **Не тестируйте через панику в no_std**: `catch_unwind` нет, упавший assert останавливает устройство.
> - **Не полагайтесь на порядок логов cargo**: compile-сообщения идут в stderr, вывод QEMU — в stdout; грепайте файл целиком (`2>&1`).
> - **Печатайте `ok` после проверок, а не до** — иначе отсутствие паники в логе ничего не значит.
> - **Не используйте время как seed** — RTC нет, а `CNTVCT` в QEMU недетерминирован; фиксированный seed воспроизводим.
> - **Не аллоцируйте в тестах**: bump-аллокатор не освобождает, а тесты не должны зависеть от порядка с `test_heap`.
> - **Инварианты — O(MAX_TASKS)**: в тестах нормально, в `yield` — нет.
> - **`assert!` с format-аргументами работает в `no_std`** (через `core`), сообщение увидит `panic_handler`; это дешёвый способ добавить контекст (`seed`, `step`).

## Связанные заметки
- [[stage5_policy_skeleton]], [[stage5_tdd]], [[stage5_scheduler]], [[no_std_testing]], [[roadmap_5_12]], [[cargo_aliases]], [[atomic_counter]], [[memory_barriers]]
