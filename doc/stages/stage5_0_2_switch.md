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

static mut STACK_A: Stack = Stack([0xA5; STACK_SIZE]);
static mut STACK_B: Stack = Stack([0xA5; STACK_SIZE]);

fn stack_top(stack: *mut Stack) -> usize {
    stack as usize + core::mem::size_of::<Stack>()
}

let top_a = stack_top(core::ptr::addr_of_mut!(STACK_A));
```

> [!warning] В edition 2024 нельзя брать ссылку на `static mut`
> `&mut STACK_A` — жёсткая ошибка (`static_mut_refs`). Используйте `addr_of_mut!`/`addr_of!`, которые дают сырой указатель, а не ссылку.
>
> Почему не `static STACK_A: Stack`: CPU будет писать в эту память через `sp`. Запись через указатель, полученный из разделяемой (не `UnsafeCell`) статики, нарушает модель алиасинга Rust. `static mut` + сырой указатель — корректный путь.

Канарейка: стек растёт вниз, поэтому рискованная зона — **нижние** адреса. Заполняем стек `0xA5` и после прогона проверяем первые байты через сырой указатель. Guard page нет: переполнение молча затрёт соседние статические данные, канарейка это только обнаружит.

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

## Проводка в `main`

- feature `schedtest` (по аналогии с `selftest`), чтобы обычный `cargo run` не менялся.
- Спавним две задачи с реальными стеками, вызываем `sched::start()`, `main` не возвращается.
- IRQ можно оставить включёнными: тик продолжает тикать, печать тиков уедет в idle/CP-задачу на 0.3.
- `println!` берёт UART-lock внутри макроса и отпускает до возврата; `yield_now()` не должен вызываться с удерживаемым guard'ом.

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
