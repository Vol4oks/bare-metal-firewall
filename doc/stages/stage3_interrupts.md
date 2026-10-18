---
tags: [этап-3, прерывания, gic, таймер, план]
aliases: [stage3, interrupts]
stage: 3
---

# Этап 3: прерывания и исключения — разбор

> [!abstract] Суть
> Четыре шага: (1) векторная таблица + печатающий обработчик, (2) тест «провокация fault», (3) GICv3, (4) ARM-таймер → первый IRQ. Порядок зафиксирован в ADR [[0003_exceptions_first]]: исключения до GIC, чтобы падения печатались. Теория — [[exceptions]].

## DoD — этап закрыт, когда

- `cargo run`: спровоцированный data abort печатается (EC, FAR, ELR), а не зависание;
- после инициализации GICv3 + таймера счётчик тиков в `main` растёт (IRQ приходят);
- всё в QEMU `virt` (GICv3), код готов к переносу на плату.

## Шаг 3.1 — Векторная таблица и VBAR_EL1 (без GIC)

**Файлы**: `src/irq.rs` (заглушка уже есть), `linker.ld`, `main.rs` (вызов `irq::init()` после MMU).

1. Таблица 16 векторов × 128 байт через `global_asm!` (или `#[naked]` + `naked_asm!` при rustc ≥ 1.90). Выравнивание **2 КБ**: `.align 11`.
2. Рабочие слоты — только «current EL, SPx»: Sync `0x200`, IRQ `0x280`, FIQ `0x300`, SError `0x380`. Остальные 12 — общий заглушка-вектор «unexpected».
3. В каждом активном векторе: сохранить контекст (x0–x30, ELR/SPSR/ESR/FAR) → вызвать Rust-обработчик → восстановить → `eret`. 128 байт тесно → трюк: в векторе сохранить только системные регистры и прыгнуть в общий trampoline, который сохранит GPR.
4. `irq::init()`: `msr vbar_el1, <адрес таблицы>`.
5. `linker.ld`: секция под таблицу с `. = ALIGN(2048);`, чтобы адрес удовлетворял VBAR.

## Шаг 3.2 — Тест «провокация fault»

В `main.rs` временная функция: `println!("about to fault")` → `write_volatile` по не отображённому адресу (например `0x1000_0000` — вне RAM `0x4000_0000–0x7FFF_FFFF` и UART). Ожидание: обработчик печатает Data Abort, EC=`0x25`, FAR=`0x10000000`, дальше паника. До шага 3.1 тот же эксперимент = тихое зависание — разница и есть результат шага.

## Шаг 3.3 — GICv3 (QEMU virt)

**Сначала MMU**: регион GIC `0x0800_0000` (2 МБ, Device) должен быть отображён — иначе первый read даёт Data Abort (см. [[gic_mapping_and_waker]]).

Адреса проверить через `dumpdtb` ([[address_sources]]): GICD `0x0800_0000`, GICR ядра 0 `0x080A_0000` (кадр: RD_base 64 КБ + SGI_base 64 КБ).

| Регистр | Смещение | Действие |
|---|---|---|
| GICD_CTLR | `0x000` | ← EnableGrp1 (бит 1) + ARE (бит 4) |
| GICR_WAKER (RD+0x14) | — | ProcessorSleep (бит 1) ← 0, ждать ChildrenAsleep (бит 2) = 0 |
| GICR_IGROUPR0 (SGI_base+0x080) | — | бит 27 ← 1 (group 1) |
| GICR_ISENABLER0 (SGI_base+0x100) | — | бит 27 ← 1 |
| GICR_IPRIORITYR (SGI_base+0x400) | байт INTID 27 | ← любое ≤ 0xFF |

Интерфейс CPU (системные регистры): `ICC_SRE_EL1.SRE` ← 1 (в QEMU обычно уже 1 после сброса — прочитать и напечатать), `ICC_PMR_EL1` ← 0xFF, `ICC_IGRPEN1_EL1` ← 1. Разрешить IRQ на CPU: `msr daifclr, #2`.

Проверка шага без источника прерываний: прочитать обратно и напечатать TYPER/IGROUPR/ISENABLER/PMR.

> [!warning] Подводные камни
> - Бит IGROUPR не поставили → прерывание остаётся group 0 → приходит как FIQ (обработчика нет → зависание).
> - `ICC_PMR_EL1` остался 0 → маска приоритета режет всё.
> - На плате RK3566 стоит GICv3 (GIC-600), но адреса другие — брать из TRM/DTB, не переносить из QEMU.

## Шаг 3.4 — ARM Generic Timer → первый IRQ

1. Прочитать `CNTFRQ_EL0` и напечатать (не хардкодить: в QEMU ~62.5 МГц, на плате иначе).
2. Виртуальный таймер CNTV (INTID 27, доступен с EL1): `CNTV_TVAL_EL0` ← freq/4 (~4 IRQ/с), `CNTV_CTL_EL0` ← ENABLE (бит 0), IMASK (бит 1) = 0. ISTATUS (бит 2) = 1, когда прерывание asserts.
3. В IRQ-векторе: `ICC_IAR1_EL1` → INTID (ожидаем 27) → перезарядить TVAL → `ICC_EOIR1_EL1` ← значение из IAR (при EOImode=0 это drop+deactivate) → инкремент счётчика (`AtomicU32` — мост к этапу 4).
4. В `main` цикл: печать счётчика и `wfi` — ждать следующего IRQ.

> [!warning] Подводные камни
> - PPI level-triggered: сработавший таймер держит линию, пока TVAL не перезаряжен → без перезарядки после EOI IRQ тут же снова = шторм тиков.
> - Забыт `daifclr` → таймер пингует, CPU молчит (проверка: `CNTV_CTL.ISTATUS` = 1).
> - Обработчик печатает в UART напрямую — ок для этапа 3, но принцип «ISR короткий» уже соблюдаем: счётчик + перезарядка, печать в `main`.

## Самопроверка

> [!question]
> - Почему 16 слотов × 128 байт, а не 4 вектора по числу классов?
> - Что случится, если не записать ICC_EOIR1?
> - Почему IGROUPR решает, IRQ это или FIQ?

## Источники
- Arm ARM (DDI 0487): vector tables, ESR_EL1, GICv3, ICC-регистры
- TRM RK3566: GIC-600 на плате
- DTB QEMU virt (`dumpdtb`): адреса GICD/GICR, узел timer

## Связанные заметки
- [[exceptions]], [[0003_exceptions_first]], [[address_sources]], [[mmu_basics]], [[uart_pl011]], [[multicore_path]]
- [[gic_v3]], [[arm_generic_timer]], [[irq_entry_exit]], [[gic_mapping_and_waker]]
