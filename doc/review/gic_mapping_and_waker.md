---
tags: [review, gic, mmu, этап-3]
aliases: [GIC mapping review, WAKER bits]
stage: 3
---

# Ревью: GIC в MMU, биты GICR_WAKER, IPRIORITYR

> [!abstract] Суть
> Шаги 3.1–3.2 закрыты. На 3.3 сначала не был отображён регион GIC (Data Abort), затем возникла паника `ChildrenAsleep not free` из-за неверных битов `GICR_WAKER`. Проверено через GDB и заголовок Linux: `ProcessorSleep` — бит 1, `ChildrenAsleep` — бит 2. Дальше нужно поправить байтовый `IPRIORITYR` и проверить CPU-интерфейс.

## Подтверждение запуском

```text
SYNC: EC=0x25 ISS=0x6 FAR=0x8000000 ELR=0x420004f4
PANIC: Data abort test at src/irq.rs:44:5
```

- `EC=0x25` — Data Abort из EL1.
- `ISS=0x6` — translation fault, level 2.
- `FAR=0x0800_0000` — первый адрес GICD.
- Причина: `REGIONS` в `mmu.rs` не содержал GIC; слот `L2[0x40]` был пуст.

После добавления региона появилась новая паника:

```text
PANIC: GICR_WAKER: ChildrenAsleep not free at src/irq.rs:73:13
```

## Находки

### Критично

1. **GIC не отображён (исправлено).** В `REGIONS` добавлен регион `0x0800_0000`, 2 МБ, Device. Один блок покрывает GICD (`0x0800_0000`) и все GICR (`0x080A_0000` … `0x0810_0000` для 4 ядер).
2. **Биты `GICR_WAKER` — 1 и 2, не 0 и 1.** `ProcessorSleep` — бит 1, `ChildrenAsleep` — бит 2 (RO); бит 0 зарезервирован. В QEMU 11 reset = `0x6`. Проверено через GDB: запись `0x4` → чтение `0x0` (редистрибьютор проснулся), запись `0x6` → чтение `0x6` (спит). Вариант с битами 0/1 оставляет `ChildrenAsleep = 1` навсегда → паника. Подтверждение: Linux `include/linux/irqchip/arm-gic-v3.h`.
3. **`IPRIORITYR` пишется не тем типом.** В коде приоритет пишется как `*mut u32` по адресу `SGI_base + 0x400 + 27 = …0x41B`. Это невыровненная 32-битная запись: для Device-памяти она даёт alignment fault. Нужен `*mut u8` со значением `0xA0`.

### Важно

4. **`GICD_CTLR`:** GDB в точке после записи показывает `0x52` — `DS` (бит 6), `ARE` (бит 4), `EnableGrp1` (бит 1). Reset = `0x50`, поэтому read-modify-write сохраняет `ARE`; отдельно добавлять бит 4 не нужно, но и терять его нельзя.
5. **`ICFGR1` для PPI 27** не проверен. По reset там level (0), что и нужно таймеру; стоит прочитать и убедиться.
6. **IRQ-слот — `unexpected`.** Первое настоящее прерывание напечатает `Unexpected exception!` и зависнет. Нужен trampoline с сохранением контекста и `eret` — см. [[irq_entry_exit]].

### Стиль

7. Чтение через `*mut u32` (`GICR_WAKER`) работает, но для чтения честнее `*const u32`.
8. `init_gic` — безопасная функция с предусловиями (MMU уже включён, GIC отображён, вызов один раз). Либо сделать `unsafe fn`, либо зафиксировать инвариант в doc-комментарии.

## Как проверялись биты WAKER

QEMU запускается с `-S -s`, GDB ставит breakpoint на строку после записи в `GICR_WAKER`, затем память читается/пишется через `set *(unsigned int *)0x080a0014 = ...` (нужно `set language c`, иначе GDB остаётся в режиме Rust).

| Запись в WAKER | Чтение | Вывод |
|---|---|---|
| `0x0` | `0x0` | бит 1 = 0 → проснулся |
| `0x1` | `0x0` | бит 0 игнорируется |
| `0x2` | `0x6` | бит 1 = 1 → уснул, бит 2 = 1 |
| `0x4` | `0x0` | бит 2 read-only, бит 1 = 0 → проснулся |
| `0x6` | `0x6` | бит 1 = 1 → спит |

## Порядок исправления

```text
irq.rs: WAKER bits 1/2 + dsb sy → GICR_IGROUPR0/ISENABLER0
        → IPRIORITYR0 (u8!) → ICC_SRE/PMR/IGRPEN1 → dsb/isb
irq_vectors.s: рабочий IRQ-слот + trampoline
main.rs: init_gic → start_timer → daifclr → wfi-цикл
```

## Связанные заметки
- [[gic_v3]], [[arm_generic_timer]], [[irq_entry_exit]], [[mmu_basics]], [[stage3_interrupts]]
