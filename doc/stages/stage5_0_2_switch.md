---
tags: [этап-5, планировщик, context-switch, asm, стеки, шаг-0.2]
aliases: [0.2, cooperative switch, sched_switch]
stage: 5
---

# Шаг 0.2: cooperative switch

> [!abstract] Суть
> 0.2 превращает policy из 0.1 в работающий механизм: `spawn` заполняет контекст, появляются статические стеки и asm (`sched_switch` + прыжок в первую задачу), `main` уходит в планировщик. Никаких событий и IRQ-переключений: только добровольный `yield`. Ключевые ловушки — edition 2024 и `static mut`, и два `&mut` в один массив задач.

## Что добавить

1. `spawn` заполняет `ctx`: `sp` и `x30`.
2. Два статических стека с канарейкой.
3. asm: `sched_switch` (сохранить/загрузить контекст) и `sched_start` (первый прыжок).
4. Под feature `schedtest` — две задачи `A`/`B`, `main` вызывает `start()`.

Устройство тестового модуля (стеки, задачи, монитор, чекпоинты) — [[stage5_schedtest]].

## Контекст новой задачи

В `spawn` после создания `Task`:

```rust
task.ctx.sp = stack_top as u64;
task.ctx.x30 = entry as usize as u64;
```

- `x19`–`x29` остаются нулями: для новой задачи это валидные «сохранённые» значения.
- `sched_switch` восстановит `x30` и сделает `ret` — для новой задачи это первый возврат, и он попадёт в `entry`. Отсюда требование `entry: extern "C" fn() -> !`.
- `sp` обязан быть 16-байтно выровнен (AAPCS64): проверяется в `spawn`.

## Стеки и edition 2024

```rust
const STACK_SIZE: usize = 16 * 1024;

#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

static mut STACK_A: Stack = Stack([0; STACK_SIZE]);
static mut STACK_B: Stack = Stack([0; STACK_SIZE]);

fn stack_top(stack: *mut Stack) -> usize {
    stack as usize + core::mem::size_of::<Stack>()
}

let top_a = stack_top(core::ptr::addr_of_mut!(STACK_A));
```

> [!warning] В edition 2024 нельзя брать ссылку на `static mut`
> `&mut STACK_A` — жёсткая ошибка (`static_mut_refs`). Используйте `addr_of_mut!`/`addr_of!`, которые дают сырой указатель, а не ссылку.
>
> Почему не `static STACK_A: Stack`: CPU будет писать в эту память через `sp`. Запись через указатель, полученный из разделяемой (не `UnsafeCell`) статики, нарушает модель алиасинга Rust. `static mut` + сырой указатель — корректный путь.

Канарейка: стек растёт вниз, поэтому рискованная зона — **нижние** адреса. Нулевой массив попадает в `.bss` и не увеличивает размер образа (`.bss` и так обнуляется в `start.s`), а магическое число записываем в основание стека **в рантайме**:

```rust
fn stack_top(stack: *mut Stack) -> usize {
    const CANARY: u64 = 0xDEAD_BEEF_0001_0001;
    unsafe { (stack as *mut u64).write_volatile(CANARY) };
    stack as usize + core::mem::size_of::<Stack>()
}
```

> [!warning] Не инициализируйте стек как `[0xA5; N]`
> Ненулевой паттерн превращает массив в `.data` и добавляет 16 КиБ на каждый стек прямо в образ. Проверка «нижние байты всё ещё нули» работает и без этого, а настоящую канарейку лучше писать в рантайме.

Guard page нет: переполнение молча затрёт соседние статические данные, канарейка это только обнаружит (и то если переполнение дойдёт до низа).

## Asm

Отдельный `src/sched_switch.s` + `global_asm!(include_str!("sched_switch.s"))` — как уже сделано для `start.s`.

```asm
// x0 = *mut TaskContext (from), x1 = *const TaskContext (to)
sched_switch:
    stp x19, x20, [x0, #0]
    stp x21, x22, [x0, #16]
    stp x23, x24, [x0, #32]
    stp x25, x26, [x0, #48]
    stp x27, x28, [x0, #64]
    stp x29, x30, [x0, #80]
    mov x9, sp
    str x9, [x0, #96]

    ldr x9, [x1, #96]
    mov sp, x9
    ldp x19, x20, [x1, #0]
    ldp x21, x22, [x1, #16]
    ldp x23, x24, [x1, #32]
    ldp x25, x26, [x1, #48]
    ldp x27, x28, [x1, #64]
    ldp x29, x30, [x1, #80]
    ret

// x0 = *const TaskContext — первый запуск, возвращаться некуда
sched_start:
    ldr x9, [x0, #96]
    mov sp, x9
    ldp x19, x20, [x0, #0]
    ldp x21, x22, [x0, #16]
    ldp x23, x24, [x0, #32]
    ldp x25, x26, [x0, #48]
    ldp x27, x28, [x0, #64]
    ldp x29, x30, [x0, #80]
    ret
```

Объявления в Rust:

```rust
unsafe extern "C" {
    fn sched_switch(from: *mut TaskContext, to: *const TaskContext);
    fn sched_start(to: *const TaskContext) -> !;
}
```

- Смещения `0/16/…/80/96` совпадают с `#[repr(C)]` и проверены const-assert'ами из 0.1.
- Сохраняются только callee-saved: caller-saved (`x0`–`x18`) компилятор не ждёт живыми после вызова.
- `sched_switch` — обычный вызов: для возобновлённой задачи он «вернётся», потому что её контекст восстанавливает `sp` и регистры.
- `sched_start` помечаем `-> !`: после `ret` управление уходит в `entry` и в этот кадр не возвращается.

> [!warning] FP/SIMD
> `CPACR_EL1.FPEN` пока выключен — `q0`–`q31` не используются. Если включите FP, контекст обязателен, иначе тихая порча вычислений.

## Два `&mut` в один массив

```rust
let from = &mut self.tasks[cur].ctx;   // &
let to   = &self.tasks[next].ctx;      // ошибка: второй borrow
```

Берём сырые указатели от массива:

```rust
let base = self.tasks.as_mut_ptr();
let from = unsafe { core::ptr::addr_of_mut!((*base.add(cur)).ctx) };
let to   = unsafe { core::ptr::addr_of!((*base.add(next)).ctx) };
unsafe { sched_switch(from, to) };
```

`addr_of_mut!`/`addr_of!` не создают ссылок, поэтому запрет на два заимствования не срабатывает. Это не «обход borrow checker ради удобства», а честное описание: asm берёт два независимых указателя, которые не пересекаются (индексы разные).

## Где живёт `Scheduler`

Задачи вызывают `yield_now()` без аргументов, значит планировщик должен быть достижим глобально:

```rust
static mut SCHED: Scheduler = Scheduler::new();
```

В 0.2 IRQ-обработчик планировщик не трогает (таймер только инкрементит `ticks`), поэтому блокировка не нужна. В 0.4, когда ISR начнёт ставить события, доступ станет `IrqSafeSpinLock` или атомарные маски — это отдельное решение.

Свободные функции-обёртки (они и будут API для задач):

```rust
pub fn yield_now() {
    let sched = unsafe { &mut *core::ptr::addr_of_mut!(SCHED) };

    let from = match sched.current() { Some(id) => id, None => return };
    let next = match sched.yield_current() { Some(id) => id, None => return };
    if next == from {
        return; // переключать нечего
    }

    let base = sched.tasks.as_mut_ptr();
    let from_ctx = unsafe { core::ptr::addr_of_mut!((*base.add(from)).ctx) };
    let to_ctx = unsafe { core::ptr::addr_of!((*base.add(next)).ctx) };
    unsafe { sched_switch(from_ctx, to_ctx) };
}

pub fn exit() -> ! {
    let sched = unsafe { &mut *core::ptr::addr_of_mut!(SCHED) };
    let base = sched.tasks.as_mut_ptr();

    match sched.exit_current() {
        Some(next) => {
            let ctx = unsafe { core::ptr::addr_of!((*base.add(next)).ctx) };
            unsafe { sched_start(ctx) }
        }
        None => loop {
            unsafe { core::arch::asm!("wfi") };
        },
    }
}

pub fn run() -> ! {
    let sched = unsafe { &mut *core::ptr::addr_of_mut!(SCHED) };
    let base = sched.tasks.as_mut_ptr();

    let first = match sched.start() {
        Some(id) => id,
        None => loop {
            unsafe { core::arch::asm!("wfi") };
        },
    };

    let ctx = unsafe { core::ptr::addr_of!((*base.add(first)).ctx) };
    unsafe { sched_start(ctx) }
}
```

`yield_now` — единственный путь с сохранением контекста; `exit`/`run` используют `sched_start` и не возвращаются.

> [!warning] IRQ внутри `sched_switch`
> Прерывания включены, и таймер может сработать между сохранением `sp` и его загрузкой. Это безопасно, пока ISR **не трогает планировщик**: кадр прерывания ляжет на текущий стек и будет снят до продолжения asm. В 0.4, когда ISR начнёт ставить события, порядок станет частью контракта (маска или атомики).

## Проводка в `main`

- feature `schedtest` (по аналогии с `selftest`), чтобы обычный `cargo run` не менялся.
- Спавним две задачи с реальными стеками, вызываем `sched::start()`, `main` не возвращается.
- IRQ можно оставить включёнными: тик продолжает тикать, печать тиков уедет в idle/CP-задачу на 0.3.
- `println!` берёт UART-lock внутри макроса и отпускает до возврата; `yield_now()` не должен вызываться с удерживаемым guard'ом.
- Модуль `scheduler` нужен только при `selftest`/`schedtest`. Если оставить его безусловным, в обычной сборке вернутся предупреждения `dead_code`. Варианты: `#[cfg(any(feature = "selftest", feature = "schedtest"))] mod scheduler;` либо оставить модульный `allow(dead_code)` до 0.3, когда планировщик станет основным циклом. Второй вариант честнее — тогда и снимать allow нужно на 0.3, а не на 0.2.

## Почему `schedtest` — отдельная feature

Не «по аналогии с `selftest`», а потому что это другой тип проверки.

| | `selftest` | `schedtest` |
|---|---|---|
| Что проверяет | policy и утилиты: `SpinLock`, heap, состояния планировщика | механизм: asm, стеки, переключение контекста |
| Возвращает управление | да (`selftest: end`, дальше тики) | нет (`-> !`, забирает CPU) |
| Момент запуска | до `gic::init`/`daifclr` | вместо основного цикла, уже с IRQ |
| Стоимость в образе | почти нулевая | 2×16 КиБ стеков + asm |
| Цена ошибки | паника в selftest | fault/triple fault в середине переключения |

Три конкретные причины:

1. **`selftest::run()` обязан вернуться**, иначе `main` не дойдёт до инициализации GIC/таймера. Встроенный `schedtest` сделал бы всё после него недостижимым, и пришлось бы прятать `selftest: end` и тики за `cfg`.
2. **`--features selftest` должен оставаться проверкой только логики.** Если сломается asm, вы всё равно увидите зелёные `sched: … ok` и поймёте, что policy цела. При объединении падение механизма маскирует результаты policy.
3. **Разные предусловия.** Selftest идёт до инициализации GIC/таймера и не должен от них зависеть; `schedtest` живёт там, где был главный цикл, с включёнными IRQ.

Альтернатива — отдельный `[[bin]]` (второй образ) — станет дешёвой только в workspace, когда `_start`, panic-handler и линковка вынесутся в общий крейт. Сейчас фича дешевле: один бинарник, один линкер-скрипт.

```bash
cargo run                                   # обычный цикл тиков
cargo run --features selftest               # policy, затем тики
cargo run --features schedtest              # планировщик вместо тиков
cargo run --features "selftest schedtest"   # policy, затем планировщик
```

Цена — четыре конфигурации сборки вместо двух. На практике проверяются три: последняя — композиция, и она же ловит ошибку порядка.

> [!warning] Порядок при обеих фичах
> `selftest` обязан идти раньше `schedtest`: второй не возвращается. Порядок в `main` должен быть явным (отдельные `cfg`-блоки), а не зависеть от порядка вызовов в тексте.

## Отладка

| Симптом | Вероятная причина |
|---|---|
| Видно только `A` | `yield_now` не переключает: `sp` не сохранён/не загружен, либо `next == from` |
| Один `A`, затем тишина | `ret` в ноль: забыт `x30 = entry` |
| `A B`, затем мусор | не выровнен `sp` или стек мал |
| `A B A` с испорченными локальными | не восстановлены `x19`–`x29` |
| `PANIC: … not aligned` | `stack_top % 16 != 0` |
| Стек «протёк» в соседние статические | переполнение; канарейка внизу, guard page нет |

GDB: `break sched_switch`, затем `info registers sp x30` и `x/13gx $x0` — сравнить контекст до и после. Брейкпоинт на `task_b` подтверждает, что первый `ret` ушёл в `entry`.

## DoD

- `A B A B …` без таймера;
- канарейки целы;
- `cargo clippy --bin bare-metal-firewall --features schedtest` и `fmt --check` чисты;
- обычный `cargo run` не изменился.

## Ловушки

> [!warning]
> - Забыли `x30 = entry` → `ret` в ноль.
> - `sp` не выровнен на 16 → fault или тихая порча кадра.
> - `&mut STACK` в edition 2024 → ошибка компиляции; только `addr_of_mut!`.
> - `sched_switch` клиффит callee-saved, которые Rust ждёт сохранёнными.
> - Переключение под `SpinLock` (UART/heap) → другая задача зависнет на этом же lock.
> - `todo!()`/`panic!()` из задачи до первого `yield` → система стоит, а выглядит как «зависло после A».

## Связанные заметки
- [[stage5_policy_skeleton]], [[context_switch]], [[irq_entry_exit]], [[a64_assembly_basics]], [[roadmap_5_12]], [[spinlock]], [[stage5_tdd]]
