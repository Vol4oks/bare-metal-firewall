---
tags: [hardware, rk3566, карта-памяти, адреса, этап-1]
aliases: [memory map, карта памяти RK3566, адреса платы]
stage: 1
---

# Карта памяти NanoPi R3S (подтверждённая)

> [!abstract] Суть
> Значения ниже сняты с самой платы (U-Boot 2025.10, лог 2026-11-11), а не взяты по аналогии с RK3568/TRM. RAM у RK3566 начинается с `0x0`, а не с `0x4000_0000` как у QEMU `virt` — отсюда разные базы линковки. Периферия (UART2, GMAC, MMC) лежит в `0xFE…`.

## RAM и области загрузчика

| Область | Адрес | Размер | Источник |
|---|---|---|---|
| DRAM физически (LPDDR4) | `0x0000_0000–0x3FFF_FFFF` | 1 ГБ | DDR-лог SPL (`Size=1024MB`), баннер (`DRAM: 1 GiB`) |
| Окно, отданное U-Boot | `0x0020_0000–0x3FFF_FFFF` | 1022 МиБ | `bdinfo` (DRAM bank), `lmb_dump_all` |
| Исключённый низ | `0x0000_0000–0x001F_FFFF` | 2 МиБ | `lmb`: memory начинается с `0x200000` |
| Точка входа U-Boot до релокации | `0x00A0_0000` | — | лог BL31 (`Entry point address`) |
| U-Boot после релокации | `0x3FF2_6000` | — | `bdinfo` (`relocaddr`) |
| Резерв U-Boot + DTB + стек | `0x3CBE_A000–0x3FFF_FFFF` | ~64 МиБ | `lmb` reserved |
| Таблицы трансляции U-Boot | `0x3FFF_0000` | — | `bdinfo` (`TLB addr`) |

> [!warning] Чему верить
> `bdinfo` показывает не «физический размер DRAM», а окно, которое mainline U-Boot отдал как `memory`-узел: первые 2 МиБ исключены. Для линковки и MMU важны оба числа: RAM физически 1 ГБ с `0x0`, но размещать свой образ в `0x0–0x1F_FFFF` не нужно.
> Проверить в Linux: `cat /proc/iomem | grep -i "System RAM"`.

## Готовые адреса из env U-Boot

| Переменная | Значение | Зачем нам |
|---|---|---|
| `kernel_addr_r` | `0x0200_0000` | **база линковки для платы** |
| `loadaddr` | `0x00C0_0800` | дефолт для `tftp`/`fatload` |
| `scriptaddr` | `0x00C0_0000` | скрипты U-Boot |
| `pxefile_addr_r` | `0x00E0_0000` | pxe |
| `ramdisk_addr_r` | `0x1218_0000` | initrd |
| `fdtoverlay_addr_r` | `0x1210_0000` | overlay |
| `kernel_comp_addr_r` | `0x0A00_0000` | распакованное ядро |
| `fdtcontroladdr` | `0x3DBE_D2C0` | управляющий DTB U-Boot |

> [!tip] Почему `0x0200_0000`
> Это `kernel_addr_r` из самого U-Boot: он гарантированно не пересекается ни с точкой входа U-Boot (`0xA00000`), ни с его relocated-образом сверху (`0x3CBE_A000+`), и кратен 2 МиБ — удобно для блоков MMU.

## Периферия

| Устройство | Адрес | Подтверждение |
|---|---|---|
| UART2 (консоль) | `0xFE66_0000` | `bdinfo`: `serial addr`, `width=4`, `shift=2`, `clock=0x16E3600` (24 МГц) |
| GMAC | `0xFE01_0000` | баннер: `eth0: ethernet@fe010000` |
| MMC (SD-слот, карта) | `0xFE2B_0000` | Linux: `mmcblk1`, U-Boot: `mmc@fe2b0000: 1` |
| MMC (eMMC-слот, пуст) | `0xFE31_0000` | Linux: `mmc0`, `Failed to initialize a non-removable card` |
| GICD | `0xFD40_0000` | Linux: MBI frame `0xfd410000` внутри GICD-региона; сверить с DTB |
| ITS | `0xFD44_0000–0xFD45_FFFF` | Linux: `ITS [mem 0xfd440000-0xfd45ffff]` |
| GICR | `0xFD46_0000` + `0x2_0000` × номер ядра | Linux: CPU0 `0xfd460000`, CPU1 `0xfd480000`, CPU2 `0xfd4a0000`, CPU3 `0xfd4c0000` |
| PCIe (RTL8111H) | `0000:01:00.0`, root complex `/pcie@fe260000` | U-Boot `pci`: vendor `0x10ec`, device `0x8168`; Linux `r8169` |

> [!success] Из Linux (2026-11-11)
> `arch_timer: cp15 timer(s) running at 24.00MHz` → `CNTFRQ_EL0` = 24 МГц, `CNTV_TVAL` для 4 Гц = 6 000 000 (влезает в `u32`).
> `CPU: All CPU(s) started at EL2` — совпадает с `SPSR = 0x3c9` из BL31.
> `00200000-3fffffff : System RAM` — тот же 2-МиБ вырез внизу, что и в `bdinfo`.
> GICR-страйд `0x2_0000` — тот же, что в QEMU, поэтому `GICR_STRIDE` из [[board_bringup]] менять не нужно.

## Что из этого следует для кода

- **Две базы линковки:** QEMU `0x4200_0000`, плата `0x0200_0000`. «Единая база» невозможна — RAM в QEMU начинается там, где у платы она заканчивается ([[load_address]]).
- **MMU:** регион RAM — `0x0..0x4000_0000` (identity), регионы устройств — `0xFD40_0000` (GICD+ITS+GICR, 2 МиБ хватает на все четыре ядра) и `0xFE60_0000` (UART2), оба выровнены на 2 МиБ. Границы RAM не выходят за 1 ГБ.
- **UART:** DW 16550 с `reg-shift=2` и 32-битным доступом; другой IP, чем PL011 в QEMU.
- **Векторы/EL:** старт на EL2 → понижение до EL1 в `_start` до любых `ICC_*_EL1`.

## Открытые вопросы

> [!question]
> - Что за резерв `0x0–0x1F_FFFF`: ATF, DDR-прошивка или «дыра» DT?
> - Какой RJ45 — WAN, какой — LAN?
> - Сколько очередей у GMAC и RTL8111H (PTP-вопросы — [[ethernet_queues_ptp]])?

## Источники
- Лог консоли платы 2026-11-11: DDR-init SPL, BL31 (TF-A v2.3), U-Boot 2025.10-OpenWrt.
- `bdinfo`, `printenv`, `lmb_dump_all` в U-Boot.
- Дополнить: `/proc/iomem` и DTB из Linux.

## Связанные заметки
- [[h0_fact_sheet]], [[board_bringup]], [[address_sources]], [[load_address]], [[qemu_adaptation]], [[gic_v3]], [[uart_pl011]]
