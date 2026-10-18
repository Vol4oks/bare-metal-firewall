---
tags: [таймер, ppi, hardware, этап-3]
aliases: [ARM Generic Timer, CNTV, CNTV_TVAL]
stage: 3
---

# ARM Generic Timer: CNTV и первый IRQ

> [!abstract] Суть
> Системный счётчик `CNTFRQ_EL0` тикает всегда; таймер — это пара «сравни значение + разреши прерывание». Виртуальный таймер EL1 (`CNTV_*`) удобен тем, что доступен с EL1 без настроек EL2, а его прерывание — PPI с INTID 27.

## Счётчик и таймеры

- `CNTFRQ_EL0` — частота системного счётчика. **Не хардкодить**: в QEMU virt ~62.5 МГц, на RK3566 значение другое (сверить с DTB/TRM).
- `CNTPCT_EL0` — физический счётчик, `CNTVCT_EL0` — виртуальный (`CNTVCT = CNTPCT - CNTVOFF_EL2`).
- Пары регистров: `CNTV_CTL_EL0`, `CNTV_TVAL_EL0`, `CNTV_CVAL_EL0` (виртуальный); `CNTP_*` (физический EL1); `CNTHP_*` (EL2); `CNTPS_*` (secure).
- Мы на EL1 non-secure — берём `CNTV_*`.

## Регистры управления

| Регистр | Биты | Смысл |
|---|---|---|
| `CNTV_CTL_EL0` | 0 ENABLE | таймер включён |
| | 1 IMASK | маска прерывания, счётчик продолжает идти |
| | 2 ISTATUS (RO) | 1 = условие выполнено, линия активна |
| `CNTV_TVAL_EL0` | 32 бита, signed | запись: `CVAL = CNTVCT + TVAL` |
| `CNTV_CVAL_EL0` | 64 бита | абсолютное значение сравнения |

Условие срабатывания: `ENABLE=1`, `IMASK=0`, `CNTVCT >= CVAL`. Прерывание **level-sensitive**: линия держится, пока условие истинно.

## Пример: 4 Гц

```text
freq = CNTFRQ_EL0              // например, 62_500_000
TVAL = freq / 4                // 15_625_000 тиков ≈ 250 мс
CNTV_CTL_EL0 = ENABLE          // бит 0 = 1, IMASK = 0
```

Как только `TVAL` досчитает до нуля, `ISTATUS=1`, PPI 27 asserts. В обработчике пишем `TVAL` заново — это сдвигает `CVAL` в будущее и снимает линию.

## Timer как модуль

После выноса GIC логично вынести таймер в `timer.rs`:

- `pub const INTID: u32 = 27;` и `pub const PRIORITY: u8 = 0xA0;` — их использует `main` при `enable_ppi`;
- `TICKS: AtomicU32` — счётчик для `main`;
- `TVAL: AtomicU32` — кэш `CNTFRQ_EL0 / HZ`, чтобы не читать `CNTFRQ` в ISR;
- `init()` — читает частоту, проверяет `!= 0`, пишет `CNTV_TVAL` и `CNTV_CTL`;
- `on_irq()` — перезаряжает `TVAL`, увеличивает `TICKS`; **EOI делает `irq.rs`**;
- `ticks()` — безопасное чтение для `main`.

Порядок в `main`: `gic::GIC.init()` → `enable_ppi(timer::INTID, timer::PRIORITY)` → `timer::init()` → `daifclr`.

> [!warning] Не читать `CNTFRQ_EL0` на каждом тике
> Частота не меняется во время работы. Читать её в ISR — лишняя работа и лишний sysreg-доступ; кэшируй `TVAL` при `init()`.

## Проверка без GIC

До включения IRQ можно убедиться, что таймер вообще идёт:

1. Напечатать `CNTFRQ_EL0` и дважды `CNTVCT_EL0` с задержкой — значения должны расти.
2. Включить таймер, подождать, прочитать `CNTV_CTL_EL0.ISTATUS` — должен стать 1.
3. Только потом настраивать GIC и делать `daifclr`.

## Подводные камни

> [!warning]
> - `TVAL` — 32-битный signed. `freq/4` для 62.5 МГц помещается, но для больших интервалов используйте `CVAL` (64 бита).
> - Не перезарядили таймер до `EOI` → шторм IRQ (level-triggered).
> - `IMASK=1` при `ENABLE=1`: таймер тикает, `ISTATUS` растёт, но IRQ не приходит.
> - `CNTVOFF_EL2` может быть не 0 — сравнивать `CNTVCT`, а не `CNTPCT`.
> - На плате частота из `CNTFRQ_EL0` может быть 0, если firmware её не запрограммировала: тогда брать из DTB (`clock-frequency` узла `timer`).
> - PPI 27 — это virtual timer. Если использовать `CNTP_*`, нужен INTID 30 (non-secure physical) и разрешение доступа из EL2.

## Источники
- Arm ARM (DDI 0487): Generic Timer, `CNTV_*`, PPI assignments
- Linux `arm_arch_timer.h`: PPI 10/11/13/14 → INTID 26/27/29/30
- DTB QEMU virt: узел `timer`

## Связанные заметки
- [[gic_v3]], [[irq_entry_exit]], [[stage3_interrupts]]
