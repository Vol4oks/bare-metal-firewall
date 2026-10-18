---
tags: [review, прерывания, vectors, этап-3]
aliases: [IRQ slot unexpected, ticks 0]
stage: 3
---

# Ревью: `ticks = 0` и `Unexpected exception!`

> [!abstract] Суть
> Таймер и GIC работают: IRQ действительно приходит. Но слот IRQ в таблице векторов всё ещё указывает на `unexpected`, поэтому первое прерывание печатает `Unexpected exception!` и уходит в вечный цикл. Нужно заменить слот на trampoline, который сохраняет контекст и возвращается через `eret`.

## Диагноз

Вывод:

```text
ticks = 0
Unexpected exception!
```

- `ticks = 0` печатается в `main` до первого `wfi`.
- Через ~250 мс таймер поднимает PPI 27.
- GIC отдаёт IRQ, CPU прыгает на `VBAR_EL1 + 0x280`.
- В слоте стоит `b unexpected` → печать и `loop`.

Проверка через GDB: breakpoint на `VBAR + 0x280` (`0x42004a80` при `VBAR = 0x42004800`) срабатывает, в нём `b unexpected`. Breakpoint на FIQ (`+0x300`) не срабатывает — значит, группа и приоритет настроены верно, прерывание приходит именно как IRQ.

## Что исправить

1. В `irq_vectors.s` заменить `SLOT unexpected` для `0x280` на `SLOT irq_handler_asm`.
2. Добавить trampoline `irq_handler_asm`: сохранить `x0–x30` и `ELR/SPSR`, вызвать Rust `irq_handler`, восстановить, `eret`.
3. `irq_handler` уже есть в `irq.rs` и возвращает `()` — это правильно; `unexpected` для IRQ не подходит, потому что он `-> !`.

Полный скелет — в [[irq_entry_exit]].

## Почему это хорошая новость

До этого шага не работали MMU-маппинг GIC, WAKER, группа/приоритет, CPU-интерфейс и таймер. Сейчас всё это уже сработало: осталась только «дверь» вектора.

## Связанные заметки
- [[irq_entry_exit]], [[gic_v3]], [[arm_generic_timer]], [[stage3_interrupts]], [[vector_table_31]]
