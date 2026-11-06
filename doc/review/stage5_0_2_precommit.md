---
tags: [review, этап-5, шаг-0.2, precommit, schedtest]
aliases: [0.2 precommit, precommit review]
stage: 5
---

# Pre-commit ревью шага 0.2

> [!abstract] Суть
> Механизм работает: cooperative switch, стеки, SIMD-контекст, композиция с `selftest`, а негативная проверка **ловит** отсутствие переключений. Блокер для коммита один — clippy не чист ни в одной из четырёх конфигураций (`dead_code` + `field_reassign_with_default`), плюс обрезанный doc-комментарий модуля. После четырёх правок шаг можно закрывать.

## Что проверено

| Проверка | Команда | Результат |
|---|---|---|
| Debug-сборка | `cargo build` | ✅ |
| Release-сборка | `cargo build --release` | ✅ |
| Форматирование | `cargo fmt --check` | ✅ |
| Обычный запуск | `cargo run` | ✅ тики, поведение прежнее |
| Policy-тесты | `cargo run --features selftest` | ✅ `states/round-robin/priority/events/model ok` |
| Стенд | `cargo run --features schedtest` | ✅ `sched: switch ok`, `sched: canary ok` |
| Композиция | `cargo run --features "selftest schedtest"` | ✅ policy → стенд |
| Символы asm | `nm … \| grep sched_` | ✅ `T sched_switch`, `T sched_start` |
| Нет заглушек | `grep -rn "todo!\|unimplemented!"` | ✅ |
| **Негативный тест** | в копии убраны все `yield_now()` | ✅ `sched: FAIL counts=true errors=18 canary=true` |
| Clippy | 4 конфигурации | ❌ см. ниже |

Негативный тест — главное: стенд больше не «слеп» к отсутствию `sched_switch`. Проверка чередования (`LAST`-атомик) работает.

## Ревью кода

Корректно:

- `spawn` заполняет `ctx.sp`/`ctx.x30`, проверяет ненулевой и 16-выровненный стек;
- `yield_now`: `next == from` не переключает; указатели на `ctx` берутся от `tasks.as_mut_ptr()`;
- `exit`/`run`: `None` → `wfi`-заглушка idle;
- asm: смещения `0…208` совпадают с `TaskContext`, SIMD сохраняется **и восстанавливается** в обеих функциях;
- `start.s`: FPEN включён до загрузки `x0` под `.bss`, `isb` на месте;
- `schedtest`: канарейки, маркеры, счётчики, лимит ожидания монитора, проверка чередования.

Замечание (не блокер): `yield_now` держит `&mut *addr_of_mut!(SCHED)` **через** вызов `sched_switch`. Практически безопасно (после вызова ссылка не используется), но хрупко: под Stacked Borrows вторая `&mut` из другой задачи инвалидирует первую. Лучше ограничить область видимости:

```rust
let (from_ctx, to_ctx) = {
    let sched = unsafe { &mut *core::ptr::addr_of_mut!(SCHED) };
    // … policy и вычисление указателей …
    (from_ctx, to_ctx)
};
unsafe { sched_switch(from_ctx, to_ctx) };
```

## Блокер: clippy

Ни одна конфигурация не чиста при `-D warnings`:

| Конфигурация | Ошибки |
|---|---|
| `default` | `spawn`/`yield_now`/`exit`/`run` never used; `Prio::DP/CP/new/get` never used |
| `--features selftest` | то же минус `DP/CP` |
| `--features schedtest` | `Prio::DP/new/get`, `Blocked`, `stack_top`/`name`, `BlockResult`, `state`/`try_wait_current`/`block_current`/`wake`/`assert_invariants` |
| `--features "selftest schedtest"` | `Prio::new/get`, `Task.name`, `ticks`, `Stack.0`, `field_reassign_with_default` |

### Причина

`dead_code` — следствие feature-гейта: policy нужна `selftest`, механизм — `schedtest`, а `ticks` используется только циклом, который выключен при `schedtest`. Строка `#[allow(dead_code)]` сейчас стоит **на `static mut SCHED`** и ничего не закрывает.

### Правки

1. В начале `scheduler.rs` (после `//!`-документации) — модульный allow, вместо атрибута на `SCHED`:

```rust
#![allow(dead_code)] // TODO(0.3): снять, когда планировщик станет основным циклом
```

2. `schedtest/mod.rs`:

```rust
#[allow(dead_code)] // поле читается только через сырой указатель в init_stack/canary_ok
#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);
```

3. `timer.rs`:

```rust
#[allow(dead_code)] // используется циклом тиков, выключенным при feature schedtest
pub fn ticks() -> u32 { … }
```

4. `spawn` — убрать `field_reassign_with_default`:

```rust
let ctx = TaskContext {
    sp: stack_top as u64,
    x30: entry as usize as u64,
    ..Default::default()
};
```

После этого проверить все четыре конфигурации:

```bash
cargo clippy --offline -- -D warnings
cargo clippy --offline --features selftest -- -D warnings
cargo clippy --offline --features schedtest -- -D warnings
cargo clippy --offline --features "selftest schedtest" -- -D warnings
```

## Косметика перед коммитом

- Первая строка `scheduler.rs`: модуль начинается с `//! атомарные события и IRQ — 0.4.` — обрезанный комментарий. Заменить на цельный: «policy (0.1) и cooperative switch (0.2); события и IRQ — 0.4».
- Комментарий над `TaskContext` всё ещё говорит «Asm появится в 0.2» — asm уже есть.
- `#[allow(dead_code)]` на `SCHED` станет лишним после модульного allow.

## Что войдёт в коммит

`git status` показывает незакоммиченными:

| Файл | Действие |
|---|---|
| `src/scheduler.rs`, `src/sched_switch.s`, `src/schedtest/`, `src/start.s`, `src/main.rs`, `Cargo.toml` | код 0.2 |
| `doc/concepts/fp_simd_in_kernel.md`, `doc/stages/stage5_schedtest.md`, `doc/review/stage5_schedtest_review.md`, `doc/index.md` | знания |
| `README.md` | решить: включить правку или откатить отдельно |

## Вердикт

**Коммитить после clippy-правок.** Функционально шаг закрыт: все режимы работают, негативный тест доказывает, что стенд не декоративный. Блокер чисто гигиенический и правится четырьмя маленькими изменениями.

## Итог

Все замечания закрыты: clippy чист во всех четырёх конфигурациях, `fmt` чист, `timer::ticks` и `Stack` получили адресные allow, `field_reassign_with_default` исправлен, модульный allow помечен `TODO(0.3)`. Заголовок модуля поправлен.

Шаг 0.2 закрыт, изменения закоммичены (`f7949a4`). Дальше — [[stage5_0_3_idle]].

## Связанные заметки
- [[stage5_0_2_switch]], [[stage5_schedtest]], [[stage5_schedtest_review]], [[fp_simd_in_kernel]], [[no_std_testing]]
