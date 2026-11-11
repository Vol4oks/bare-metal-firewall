---
tags: [hardware, rk3566, bringup, факты, этап-1]
aliases: [H0, факт-лист платы, board facts]
stage: 1
---

# H0: факт-лист NanoPi R3S

> [!abstract] Суть
> Пока факты о плате не подтверждены её же консолью, `board.rs`, линкер и `_start` остаются QEMU-ными. Это рабочий лист H0: какой факт нужен, зачем он в коде, какой командой добывается. Статус: **половина собрана 2026-11-11** — U-Boot отвечает, закрыты DRAM, EL, UART и носители; остались GIC, `CNTFRQ`, PCIe и порты.

> [!info] Что уже известно
> - Плата грузится с SD: SPL → BL31 → U-Boot 2025.10 (OpenWrt), консоль UART2 ожила.
> - Нет eMMC/NAND; без SD BootROM уходит в Maskrom и UART молчит — это норма, а не неисправность ([[rk3566_bootrom_media]]).
> - Maskrom подтверждён: ПК видит `2207:350a` (Fuzhou Rockchip Electronics).
> - Кнопка `MASK` есть, SD-карта и ридер есть → путь A доступен, порядок — [[sd_card_image]].

## Подтверждено логом U-Boot (2026-11-11)

| # | Факт | Значение | Источник |
|:--:|---|---|---|
| 1 | DRAM | LPDDR4 1 ГБ; банк с `0x0000_0000`, окно `0x0020_0000–0x3FFF_FFFF` (1022 МиБ) | DDR-лог SPL, `bdinfo` |
| 2 | База для своего образа | `0x0200_0000` (`kernel_addr_r`); `relocaddr` = `0x3FF2_6000`; U-Boot резервирует `0x3CBE_A000–0x3FFF_FFFF` | `bdinfo`, lmb |
| 3 | EL после загрузчика | **EL2h**, DAIF замаскированы (`SPSR = 0x3c9`) | лог BL31 |
| 4 | Консоль | UART2 `0xFE66_0000`, width 4, shift 2, clock 24 МГц, 1500000 8N1 | баннер, `bdinfo` |
| 5 | SPI NOR | не обнаружен: `flashsize = 0`, env «from nowhere» → `saveenv` некуда | `bdinfo`, `printenv` |
| 6 | Носители | SD на `mmc@fe2b0000` (карта есть), `mmc@fe310000` пуст; SPL грузился по FIT с MMC2 | баннер, лог SPL |
| 7 | Ethernet | GMAC `eth0` = `ethernet@fe010000`; RTL8111H в списке нет — нужен PCIe | баннер `Net:` |
| 8 | Модель/DT | `FriendlyElec NanoPi R3S`; `fdtfile = rockchip/rk3566-nanopi-r3s.dtb` | баннер, `printenv` |
| 9 | U-Boot | 2025.10-OpenWrt; `board = evb_rk3568`; env 711/126972 Б (встроенный) | `bdinfo`, `printenv` |
| 10 | GIC | GICD `0xFD40_0000`; ITS `0xFD44_0000`; GICR `0xFD46_0000`, страйд `0x2_0000` | Linux `dmesg` |
| 11 | Таймер | `CNTFRQ_EL0` = 24 МГц (`TVAL` для 4 Гц = 6 000 000) | Linux `dmesg`: `arch_timer … 24.00MHz` |
| 12 | Порты | `eth0` = GMAC + PHY RTL8211F; `eth1` = RTL8111H по PCIe `01:00.0` (`10ec:8168`), MAC не прошит | `pci`, `dmesg` |
| 13 | Роли | `eth0` (GMAC) — WAN, порт **у USB-C**; `eth1` (8111H) — LAN в `br-lan`; PCIe — Gen.1 x1 | `dmesg`, `br-lan`, осмотр |

Полная карта адресов — [[memory_map_rk3566]]; порты — [[ethernet_queues_ptp]].

## Осталось

| Факт | Где взять |
|---|---|
| Сколько RX/TX-очередей, есть ли PTP-метки | TRM GMAC + datasheet RTL8111H ([[ethernet_queues_ptp]]) |
| Что занимает `0x0–0x1F_FFFF` | DTB (`/sys/firmware/fdt`) |
| Какая физическая наклейка/сторона у WAN-порта | известно: WAN — ближайший к USB-C; для TFTP годится только он (в U-Boot есть только GMAC) |

## Зачем эти факты

| # | Факт | Что от него зависит | Ошиблись → симптом |
|:--:|---|---|---|
| 1 | DRAM: база и размер | база линковки, регион RAM в MMU | образ не там, где ждёт `go`; мусор/прыжок в ноль |
| 2 | `relocaddr`, `kernel_addr_r`, `loadaddr` | выбор безопасной базы для своего образа | затираем relocated U-Boot → плата «умирает» до перезагрузки |
| 3 | EL после `go` (EL1/EL2/EL3) | понижение EL в `_start` | trap `EC=0x18` на `ICC_SRE_EL1`, уход в ноль |
| 4 | Консольный UART (база, `reg-shift`) | драйвер `Dw16550`, `mmu.rs` | «Hello» некуда писать |
| 5 | Наличие SPI NOR и размер | где живёт U-Boot/env, план восстановления | неверный `saveenv`, потеря env |
| 6 | Носители: SD / USB / сеть | путь H1 (чем грузить образ) | нет способа доставить образ |
| 7 | Какое устройство — GMAC, какое — RTL8111H | этап 6, роли WAN/LAN, [[0007_core_partitioning]] | драйвер не для того порта |
| 8 | GICD/GICR: базы и страйд GICR | `board.rs`, `gic.rs` | PPI 27 не включается, `ticks` стоят |
| 9 | `CNTFRQ_EL0` | делитель таймера | тики не 4 Гц (или `TVAL = 0`) |
| 10 | `CurrentEL` и `MPIDR` на вторичных ядрах | парковка, per-core GIC | fault на припаркованном ядре |

## H0a: баннер (ничего не нажимая)

1. Подключить консоль — [[serial_console]] (3.3 В, GND, TX↔RX, 1500000 8N1).
2. Открыть лог, **подать питание** и не трогать клавиатуру до конца автозагрузки.
3. Сохранить весь лог: он отвечает сразу на пункты 1, 2, 5, 6, 7.

Что искать в баннере:

| Строка в логе | Факт |
|---|---|
| `U-Boot 20xx.xx` | загрузчик есть, версия (от неё зависит набор команд) |
| `DRAM: 1 GiB` | размер RAM подтверждён |
| `Loading Environment from SPIFlash` / `from MMC` / `*** Warning - bad CRC` | где живёт env (п. 5) |
| `MMC: mmc@fe2b0000: 0` / `no card present` | есть ли SD (п. 6) |
| `SF: Detected <chip> ... 16 MiB` | SPI NOR есть, размер (п. 5) |
| `Net: eth0: ethernet@…, eth1: rtl8169#0` | GMAC и RTL8111H видны, их имена (п. 7) |
| `Hit any key to stop autoboot` | дальше можно прервать загрузку |

> [!tip] DT-имена несут адреса
> Узлы вида `serial@fe660000`, `ethernet@…`, `mmc@…` — это уже подтверждение базы устройства, даже без `dtc`. `printenv stdout` показывает именно такой узел.

## H0b: команды U-Boot

Перезагрузить, прервать автозагрузку, снять лог:

```text
version
bdinfo
printenv
mmc info
mmc part
sf probe
usb start
pci enum
pci
```

Пояснения и что смотреть:

| Команда | Что выписать |
|---|---|
| `version` | версия/сборка U-Boot, дата |
| `bdinfo` | `DRAM bank` — **база** и `size`; `relocaddr`; `fdt_blob` |
| `printenv` | `loadaddr`, `kernel_addr_r`, `fdt_addr_r`, `fdtfile`, `stdout`/`stderr`/`stdin`, `bootcmd`, `boot_targets` |
| `mmc info` / `mmc part` | есть ли карта, таблица разделов |
| `sf probe` | SPI NOR: есть/нет, объём |
| `usb start` | USB-хост работает? есть ли флешка? |
| `pci enum` + `pci` | видно ли RTL8111H (vendor:device)

> [!warning] Команды могут отсутствовать
> `sf`, `pci`, `usb`, `net` — опциональные (`CONFIG_CMD_*`). Сообщение «Unknown command» — тоже факт: функция не собрана, и путь через неё закрыт.

### Попытка достать DTB из U-Boot

Если U-Boot собран с `OF_CONTROL`, его управляющий DTB лежит в RAM:

```text
printenv fdtcontroladdr
fdt addr ${fdtcontroladdr}
fdt print /soc
```

Вывод большой; нужны узлы `serial@…`, `interrupt-controller@…` (или `gic@…`), `ethernet@…`, `memory@…`. Если `fdtcontroladdr` нет — пропустить, факты брать из TRM.

## H0c: чего U-Boot не расскажет

| Факт | Почему | Как добыть |
|---|---|---|
| EL после `go` | U-Boot не печатает свой EL | свой минимальный образ (H2, [[board_bringup]]) или Linux `dmesg` |
| `CNTFRQ_EL0` | не его дело | Linux: `arch_timer: cp15 timer(s) running at 24.00MHz`; иначе — H2 |
| Базы и страйд GIC | U-Boot ими почти не пользуется | TRM RK3566 / DTB платы; страйд — `GICR_TYPER` в рантайме |
| Какой RJ45 — WAN, какой — LAN | зависит от платы, не от SoC | кабель + link LED / `ethtool` / схема платы |

## Что меняется в коде по итогам

| Установленный факт | Правка |
|---|---|
| DRAM с `0x0`, 1 ГБ | `linker-rk3566.ld` с базой вроде `0x0200_0000` (сверить с `relocaddr` и `kernel_addr_r`), MMU: RAM `0x0..0x4000_0000` |
| RAM, как в QEMU, с `0x4000_0000` | можно оставить общий линкер, но проверить пересечение с `relocaddr` |
| U-Boot оставляет EL2 | `_start`: `HCR_EL2.RW`, `SPSR_EL2`, `ELR_EL2`, `eret` до любых `ICC_*_EL1` |
| Консоль = UART2 | trait `Console`; `Pl011` (QEMU) и `Dw16550` (плата, `reg-shift = 2`) |
| GICD `0xFD40_0000`, GICR `0xFD46_0000` | значения в `board.rs` под feature `board-rk3566` |
| Страйд GICR ≠ `0x2_0000` | `GICR_STRIDE` в `board.rs`, `gicr_for_core` считает по нему |
| `CNTFRQ` = 24 МГц | `TVAL = 24e6 / HZ`; проверить, что влезает в `u32` |
| SPI NOR есть | восстановление возможно без SD; `saveenv` — только осознанно |

## Подводные камни

> [!warning]
> - **`relocaddr` — это U-Boot, а не место для нас.** Свой образ должен лежать ниже relocated U-Boot и выше TF-A/OP-TEE. Брать `kernel_addr_r` как ориентир, а не «первую свободную дырку».
> - **`go` портит `x0`.** Аргумент «указатель на DTB» из `_start` не подхватить; адрес DTB придётся передавать вручную или не полагаться на него.
> - **Молчание ≠ одна причина.** Адрес, EL, UART — три независимые версии. Изолировать по одному, а не менять всё сразу (H2 для этого и нужен).
> - **Не делать `saveenv` на этом шаге.** Сначала `printenv` в файл и понимание, где env живёт.
> - **Кэши/MMU от загрузчика.** U-Boot может оставить их включёнными; в `_start` выключать явно, до первого обращения к памяти «по-своему».
> - **Скорость 1500000 на CH340** — частая причина «мусора»; см. [[serial_console]].

> [!question] Самопроверка
> - Чем `relocaddr` отличается от `loadaddr` и почему образ нельзя класть «в любое свободное место»?
> - Почему базу MMU-региона устройств надо брать выровненной на 2 МиБ, а не равной базе UART?

## Как ведём журнал

- Сырые логи — в `doc/hardware/logs/` (`h0a-banner.log`, `h0b-uboot-cmds.log`), как есть.
- Значения — в таблицу «Факты» выше, с источником (`bdinfo`, баннер, TRM стр., H2).
- Пункт считается закрытым, только когда указан **источник** и значение; «примерно так» не закрывает.

## Источники
- TRM RK3566: главы UART, GMAC, GIC, CRU (для `CNTFRQ`).
- U-Boot: `doc/usage/cmd/bdinfo.rst`, `printenv`, `fdt`, `sf`, `pci`.
- ARM ARM (DDI 0487): `CurrentEL`, `HCR_EL2.RW`, `SPSR_EL2`, `CNTFRQ_EL0`.
- Wiki FriendlyElec (NanoPi R3S): консоль UART2, 1500000 8N1.

## Связанные заметки
- [[memory_map_rk3566]], [[board_bringup]], [[serial_console]], [[sd_card_image]], [[rk3566_bootrom_media]], [[load_address]], [[address_sources]], [[qemu_adaptation]], [[ethernet_queues_ptp]], [[0007_core_partitioning]], [[gic_v3]], [[arm_generic_timer]]
