---
tags: [hardware, rk3566, bringup, nanopi, план, этап-1]
aliases: [board bringup, запуск на плате, H0-H7]
stage: 1
---

# Bring-up NanoPi R3S (RK3566)

> [!abstract] Суть
> Плата в руках, но код вырос на QEMU: другой адрес загрузки, другой UART (DW 16550 вместо PL011), другой уровень исключений после U-Boot (EL2), другие базы GIC и MMU. План — от «собрать факты» до «тики на железе», с минимальным образом на каждом шаге. Главный риск — молчание в UART: он же симптом трёх разных ошибок, поэтому UART проверяется отдельно и первым.

## Что уже готово и что расходится

| Компонент | QEMU | NanoPi R3S (RK3566) | Что делать |
|---|---|---|---|
| Адрес загрузки | `0x42000000` (RAM с `0x40000000`) | RAM, вероятно, с `0x0`, 1 ГБ | **две базы линковки** |
| Уровень после загрузчика | EL1 | U-Boot под TF-A обычно EL2 | drop EL2 → EL1 |
| UART | PL011 `0x09000000` | DW 16550, UART2 `0xFE660000`, `reg-shift=2` | новый драйвер |
| GIC | GICv3 `0x08000000` | GICv3 (GIC-600) `0xFD400000` / `0xFD460000` | базы + страйд |
| MMU | RAM 1G, GIC 2M, UART 2M | RAM с `0x0`, GIC 2M, UART 2M | новые регионы + выравнивание |
| Таймер | `CNTV`, PPI 27, 62.5 МГц | `CNTV`, PPI 27, вероятно 24 МГц | проверить `CNTFRQ` |
| Кэши/MMU до нас | выключены | U-Boot может оставить включёнными | явно выключить в `_start` |

## Факт №1: адрес загрузки

`linker.ld` линкует на `0x42000000`, и [[load_address]] утверждал, что адрес годится и для платы. **Проверено 2026-11-11: не годится.** U-Boot сообщает DRAM с `0x0`, окно `0x0020_0000–0x3FFF_FFFF` (1022 МиБ), то есть `0x42000000` — вне RAM. У QEMU `virt` RAM начинается с `0x40000000` — «единой базы» не существует.

Проверить в U-Boot:

```text
bdinfo          # DRAM bank = 0x…, size = 0x…
printenv        # kernel_addr_r, fdt_addr_r, loadaddr
```

Проверить в вендорском Linux:

```bash
cat /proc/iomem | grep -i "System RAM"
dmesg | grep -iE "memory|dram"
```

> [!warning] Правило
> `loadaddr` в `tftp`/`fatload` **обязан** совпадать с базой линковки, иначе `ldr =symbol` и literal pool указывают мимо: мусор в UART или прыжок в ноль. База подтверждена: **`0x0200_0000`** — это `kernel_addr_r` этого U-Boot; она выше точки входа U-Boot (`0xA00000`), ниже резерва (`0x3CBE_A000+`) и кратна 2 МиБ. Подробно — [[memory_map_rk3566]].

Следствие: **две базы**. QEMU — `0x42000000`, плата — `0x02000000`. Реализация — отдельные `linker-qemu.ld` / `linker-rk3566.ld` и выбор по feature `board-rk3566` (см. «Изменения в коде»).

## Факт №2: уровень исключений

`go` прыгает на образ в том EL, в котором работает U-Boot. С TF-A это обычно EL2. Наш код использует `ICC_*_EL1`, `VBAR_EL1`, `CPACR_EL1`: первые доступны **только** с EL1. На EL2 первое же обращение к `ICC_SRE_EL1` даст trap `EC=0x18`, а `VBAR_EL2` у нас не настроен → уход в ноль.

> [!success] Проверено 2026-11-11
> BL31 печатает `SPSR = 0x3c9`: `M[3:0] = 0b1001` = **EL2h**, `DAIF = 1111` — прерывания замаскированы. Значит `go` стартует наш образ на **EL2**, и понижение до EL1 обязательно до первого `ICC_*_EL1`.

Скелет понижения EL в `_start` (после маски прерываний, до любых sysreg GIC):

```asm
    mrs x0, currentel
    cmp x0, #(2 << 2)          // EL2?
    b.ne 1f

    mov x1, #(1 << 31)         // HCR_EL2.RW = 1: EL1 — AArch64
    msr hcr_el2, x1
    mov x1, #0x3c5             // SPSR_EL2: EL1h + DAIF masked
    msr spsr_el2, x1
    adr x1, 1f
    msr elr_el2, x1
    isb
    eret
1:
```

Если окажется EL3 (U-Boot без TF-A), понадобится аналогичный переход EL3 → EL2 → EL1 или сразу EL3 → EL1 через `SCR_EL3`/`SPSR_EL3`/`ELR_EL3`. Первым делом стоит напечатать `CurrentEL` — но печать возможна только когда UART уже работает, поэтому в минимальном образе H2 выводим и EL, и байт-маркер.

## Факт №3: UART — другой IP

Rockchip UART — DesignWare 16550-совместимый: `reg-shift = <2>`, `reg-io-width = <4>`, то есть регистр `i` лежит по `base + i*4`, доступ 32-битный.

| Регистр | Смещение | Что нужно |
|---|---|---|
| THR | `0x00` | писать байт |
| LSR | `0x14` | бит 5 (`THRE`) — можно писать |

```rust
const UART: usize = 0xFE66_0000; // UART2 — подтверждено bdinfo 2026-11-11
const THR: usize = 0x00;
const LSR: usize = 0x14;
const THRE: u32 = 1 << 5;

unsafe {
    let lsr = (UART + LSR) as *const u32;
    let thr = (UART + THR) as *mut u32;
    while lsr.read_volatile() & THRE == 0 {}
    thr.write_volatile('H' as u32);
}
```

> [!warning] Не инициализируем CRU и пинмукс
> Консольный UART уже настроен U-Boot (клок, пинмукс, 1500000 8N1). На первом шаге мы только пишем в THR — это экономит целый пласт bring-up. Если понадобится другой UART или другая скорость, придётся разбираться с CRU/GRF.

PL011-драйвер остаётся для QEMU; нужен второй драйвер и общий trait (`Console`), чтобы `println!` не знал, какой IP под ним.

## GIC: базы и страйд

> [!success] Подтверждено на плате (2026-11-11)
> ```text
> GICD 0xFD40_0000     (MBI frame 0xFD41_0000 внутри региона)
> ITS  0xFD44_0000     (128 КиБ)
> GICR 0xFD46_0000     страйд 0x2_0000: CPU0 fd460000, CPU1 fd480000, CPU2 fd4a0000, CPU3 fd4c0000
> ```
> Страйд совпадает с QEMU, так что `GICR_STRIDE` под плату менять не нужно — достаточно баз в `board.rs`. Источник: `dmesg | grep -iE "gic|redistributor"`.

В DTS у `gic` размер региона GICR делят на число ядер, но в рантайме надёжнее прочитать `GICR_TYPER`:

- `bits[23:19]` — Processor_Number;
- `bit[4]` — Last.

Идём по кандидатным базам `gicr + n*stride`, пока не упрёмся в `Last`. Если страйд угадан неверно, PPI 27 не включится и `ticks` не пойдут — при этом UART работает, что сужает поиск.

## MMU: регионы для платы

| Регион | VA | Размер | Тип |
|---|---|---|---|
| RAM | `0x0` | 1 ГБ | Normal |
| GIC (GICD+GICR) | `0xFD40_0000` | 2 МиБ | Device |
| UART | `0xFE60_0000` | 2 МиБ | Device |

> [!warning] Блок 2 МиБ должен быть выровнен
> Сейчас `mmu.rs` для `Size2M` берёт `va` как есть. `UART_BASE = 0xFE66_0000` не кратен 2 МиБ — дескриптор станет невалидным. Нужно либо передавать выровненную базу региона (`0xFE60_0000`), либо маскировать `va & !0x1F_FFFF` в `Size2M`.

## Нет SD: где живёт U-Boot

Почему при отсутствии носителя консоль молчит и как это проверить — [[rk3566_bootrom_media]].

Отсутствие карты не мешает нашему образу: он всё равно живёт в RAM. Носитель нужен **загрузчику**, а не нам.

Дерево решений:

1. **Подать питание с подключённым UART.** Если в консоли есть баннер U-Boot — загрузчик уже есть (вероятно, SPI NOR), и SD не нужна: грузимся по TFTP или с USB-флешки.
2. **Тишина** → попробовать Maskrom: зажать кнопку Maskrom, подключить USB к ПК, отпустить. `lsusb` должен показать устройство Rockchip (`2207:…`), `rkdeveloptool ld` — его же.
   - Если на плате есть **SPI NOR**, U-Boot можно прошить туда вендорским лоадером (`upgrade_tool`/`rkdeveloptool`). Раскладку и образ брать у FriendlyElec, не собирать наугад.
   - Если SPI NOR нет и SD нет — U-Boot негде хранить: либо SD-карта, либо каждый раз Maskrom-лоадер (он даёт DDR и USB-прошивку, но не UART-консоль).
3. **Практический вывод:** SD-карта 8–16 ГБ сильно упрощает цикл и остаётся точкой восстановления; Maskrom — страховка, а не ежедневный путь.

### Каналы загрузки без SD

| Канал | Как | Ограничения |
|---|---|---|
| **TFTP** (лучший) | `setenv ipaddr …; setenv serverip …; tftp <base> fw.bin; go <base>` | нужен Ethernet и настроенный хост |
| **USB-флешка** | `usb start; fatload usb 0:1 <base> fw.bin; go <base>` | нужен U-Boot с USB-хостом |
| **Maskrom + loader** | `rkdeveloptool db MiniLoaderAll.bin` | только прошивка/восстановление, свой образ напрямую не запускает |

В обоих «загрузочных» каналах адрес `tftp`/`fatload` **обязан** совпадать с базой линковки.

> [!warning] Лоадер для Maskrom
> `MiniLoaderAll.bin` должен соответствовать SoC и типу DRAM (RK3566 + LPDDR4/4X). Неподходящий лоадер падает на инициализации DDR. Брать вендорский из вики FriendlyElec.

> [!question] Что проверить первым
> Есть ли на R3S LTS SPI NOR и прошит ли туда U-Boot? Самый быстрый тест — включить плату с UART: если U-Boot печатает баннер, вопрос закрыт.

## План H0–H7

| Шаг | Цель | Проверка |
|---|---|---|
| **H0** | Собрать факты о плате | `bdinfo`, `printenv`, `/proc/iomem`, DTB |
| **H1** | Научиться грузить образ | `tftp`/`fatload` (SD или USB) + `go` по адресу линковки |
| **H2** | Первый байт в UART | минимальный образ: EL + `H` в цикле |
| **H3** | EL1 и исключения | сознательный fault печатает `ESR/FAR/ELR` |
| **H4** | GIC + таймер | `ticks` растут ~4 Гц |
| **H5** | MMU | после включения MMU UART продолжает печатать |
| **H6** | Планировщик | idle/тикер/`schedtest` на железе |
| **H7** | Регресс QEMU | обе сборки зелёные, smoke в QEMU не сломан |

### H0: факты (сделать первым)

Рабочий лист с таблицей фактов, статусами и последствиями для кода — [[h0_fact_sheet]]; настройка и диагностика консоли — [[serial_console]].

U-Boot (консоль 1500000 8N1):

```text
version
bdinfo
printenv
mmc info
mmc part
fatls mmc 0:1
sf probe            # есть ли SPI NOR
```

Linux (вендорский образ с SD):

```bash
cat /proc/iomem | grep -i "System RAM"
cat /proc/cpuinfo | head
dmesg | grep -iE "started at EL|memory|gic|uart|timer"
dtc -I fs -O dts /proc/device-tree > /tmp/board.dts
grep -nE "serial[0-9]|interrupt-controller@|memory@|clock-frequency" /tmp/board.dts
```

Что выписать: DRAM base/size, `relocaddr`, `kernel_addr_r`, база UART и `stdout-path`, базы GICD/GICR и размер GICR, `CNTFRQ` (24 МГц?), EL, где живёт U-Boot env, есть ли SPI NOR.

### H2: минимальный образ

Цель — изолировать адрес загрузки, EL и UART от всего остального:

1. `_start`: маска прерываний → понижение EL (если EL2) → стоп.
2. Прямая запись `'H'`, `'0'+EL`, `'\r'`, `'\n'` в UART2 через THR/LSR.
3. `loop { wfi }`.

Никакой MMU, GIC, кучи, планировщика. Если пошло — три главных неизвестных закрыты, и можно включать остальное по одному.

Готовый рецепт (отдельный крейт `probe/`, линкер на `0x02000000`, код `start.s`/`main.rs`, сборка через `RUSTFLAGS`, доставка TFTP, таблица отказов) — [[h2_minimal_image]].

### H5: MMU

Порядок: включить MMU без кэшей (I/C оставить выключенными), проверить, что UART печатает; затем включить кэши отдельным шагом. DMA пока нет, cache maintenance не нужен.

## Изменения в коде

| Файл | Что |
|---|---|
| `board.rs` | два набора констант: QEMU и `rk3566` (feature `board-rk3566`); выровненные базы регионов для MMU |
| `uart.rs` | trait `Console` + `Pl011` (QEMU) и `Dw16550` (плата, 32-битный доступ, `reg-shift=2`) |
| `gic.rs` | базовые адреса и страйд из `board`; остальное без изменений |
| `mmu.rs` | регионы из `board`; маска `va` до 2 МиБ для `Size2M` |
| `start.s` | проверка `CurrentEL` и drop EL2→EL1; явное выключение MMU/кэшей; FPEN (уже есть) |
| `linker.ld` | база по плате: либо два скрипта, либо `--defsym` |
| `build.rs` | выбрать линкерный скрипт по feature и передать `-T` абсолютным путём |
| `justfile` | рецепты `board-build`, `board-bin` (objcopy), `board-load` (подсказка команд U-Boot) |

Рекомендуемая форма `board.rs` — два подмодуля с одинаковыми именами констант:

```rust
#[cfg(feature = "board-rk3566")]
mod imp { pub const RAM_BASE: usize = 0x0000_0000; /* … */ }

#[cfg(not(feature = "board-rk3566"))]
mod imp { pub const RAM_BASE: usize = 0x4000_0000; /* … */ }

pub use imp::*;
```

Это же — заготовка под `fw-bsp-qemu` / `fw-bsp-rk3566` из [[0008_workspace_layout]].

## Безопасность и восстановление

- Ничего не пишем во флеш: только `fatload`/`tftp` + `go`. SD-загрузка остаётся целой.
- `saveenv` — только осознанно и после `printenv` в файл. Если env испорчен, восстановление — перепрошивка SD на ПК.
- USB-UART: **3.3 В**, TX↔RX перекрёстно, GND обязателен. 5 В на UART-пины не подавать.
- Если после `go` плата молчит — выключить питание, не гадать. Молчание = либо адрес, либо EL, либо UART.
- Худший сценарий (U-Boot затёрт) — восстановление через Maskrom; на этой плате не требуется, пока образ живёт в RAM.

## Открытые вопросы

> [!question] Осталось выяснить
> - Базы GICD/GICR и страйд GICR — из DTB или Linux-`dmesg` (U-Boot их не показывает).
> - `CNTFRQ_EL0`: `clock = 24 МГц` у UART — только намёк; нужен `dmesg | grep arch_timer`.
> - Виден ли RTL8111H: `pci enum; pci` в U-Boot или `lspci` в Linux.
> - Какой RJ45 — WAN, какой — LAN (кабель + link LED).

> [!success] Закрыто 2026-11-11
> DRAM с `0x0` (окно `0x0020_0000–0x3FFF_FFFF`); EL2 после BL31 (`SPSR = 0x3c9`); UART2 `0xFE66_0000` (width 4, shift 2); SPI NOR не обнаружен (`flashsize = 0`, env «from nowhere»); GMAC `0xFE01_0000`; SD на `mmc@fe2b0000`. Подробности — [[memory_map_rk3566]].

## Связанные заметки
- [[memory_map_rk3566]], [[h0_fact_sheet]], [[serial_console]], [[qemu_adaptation]], [[load_address]], [[address_sources]], [[uart_pl011]], [[gic_v3]], [[mmu_basics]], [[arm_generic_timer]], [[uboot_loading]], [[0008_workspace_layout]]
