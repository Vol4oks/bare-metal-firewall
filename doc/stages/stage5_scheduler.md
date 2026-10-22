---
tags: [этап-5, планировщик, задачи, контекст]
aliases: [stage5, scheduler plan]
stage: 5
---

# Этап 5: планировщик

> [!abstract] Суть
> Сначала cooperative round-robin: две задачи, `yield_now`, статические стеки. Затем preemptive: таймер, quantum и переключение на выходе из IRQ. Idle-задача — `wfi`.

## DoD — этап закрыт, когда

- две задачи по очереди печатают `A`/`B` и не падают;
- у каждой задачи свой стек и сохранённый контекст;
- `yield_now()` переключает на следующую Ready-задачу;
- (следующий milestone) таймер вытесняет задачу по quantum;
- idle-задача уходит в `wfi`;
- `cargo clippy` чист, README обновлён.

## Milestones

### 5.1 Cooperative

- `TaskContext`, `Task`, `TaskState`.
- `sched_switch` на asm.
- Две статические задачи и два статических стека.
- `sched::spawn`, `sched::yield_now`, `sched::start`.
- `main` вызывает `sched::start()` и больше не возвращается.
- Задачи:

```rust
extern "C" fn task_a() -> ! {
    loop {
        println!("A");
        sched::yield_now();
    }
}

extern "C" fn task_b() -> ! {
    loop {
        println!("B");
        sched::yield_now();
    }
}
```

Ожидаемо: `A B A B ...`.

### 5.2 Idle и exit

- Idle-задача: `loop { wfi }`.
- `task_exit()` переводит задачу в Exited и выбирает следующую.
- Если Ready-очередь пуста — idle.

### 5.3 Preemptive

- Поднять частоту таймера: 4 Гц слишком мало для quantum; 100 Гц/1 кГц удобнее.
- `timer::on_irq()` только decrement quantum и ставит `NEED_RESCHED`; ISR не печатает и не аллоцирует.
- На выходе из IRQ, если `NEED_RESCHED`, переключить задачу.
- Для этого придётся изменить IRQ-trampoline: сохранить `SP` текущей задачи, загрузить кадр следующей и сделать `eret`.

## Память под стеки

Самый простой вариант:

```rust
#[repr(align(16))]
struct Stack([u8; 16 * 1024]);

static mut STACK_A: Stack = Stack([0; 16 * 1024]);
static mut STACK_B: Stack = Stack([0; 16 * 1024]);
```

Верх стека — `base + size`, выровнен вниз до 16. Позже можно выделять стеки из heap до старта планировщика; освобождать их не нужно.

## Синхронизация

- Очередь задач читает и планировщик, и IRQ-тик. Защищать маской IRQ или `IrqSafeSpinLock`.
- Не вызывать `yield_now()` под `SpinLock` от UART/heap: другая задача может зависнуть на этом lock.
- В тике не аллоцировать и не печатать.
- Если задача вытеснена, удерживая UART-lock, другая задача будет крутиться; оригинал рано или поздно вернётся, но это priority inversion. Критические секции — короткие.

## Тесты

- Кооперативно: `A B A B ...` без таймера.
- Preemptive: задачи без `yield`, но вывод чередуется по тикам.
- Idle: без Ready-задач — `wfi`, тики продолжают идти.
- Canary в конце стека: проверять, что не затёрт.
- `cargo clippy`, `cargo fmt`.

## Подводные камни

> [!warning]
> - Стек не выровнен / слишком мал.
> - Забыли `SP` или `x30` в контексте.
> - `sched_switch` clobber'ит callee-saved регистры.
> - Переключение под lock'ом.
> - Preemptive switch внутри Rust-функции без asm-trampoline.
> - Таймер 4 Гц даёт quantum 250 мс — слишком грубо.
> - `wfi` при замаскированных IRQ = вечный сон.

## Связанные заметки
- [[task_and_scheduler]], [[context_switch]], [[irq_entry_exit]], [[arm_generic_timer]], [[spinlock]], [[multicore_path]]
