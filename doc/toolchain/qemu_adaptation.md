---
tags: [qemu, toolchain, rk3566]
aliases: [virt vs RK3566, адаптация QEMU]
stage: 1
---

# Адаптация QEMU virt под RK3566

> [!abstract] Суть
> В mainline QEMU нет машины для RK3566. Стратегия: настроить `virt` флагами так, чтобы совпадали уровни, которых касается наш код (CPU, GIC, RAM, адрес загрузки), а расходящиеся драйверы периферии (UART, Ethernet) спрятать за абстракцию платы.

## Расхождения и флаги

| Уровень | QEMU virt | NanoPi R3S (RK3566) | Флаг/решение |
|---|---|---|---|
| CPU | `-cpu max`, 1 ядро | 4× Cortex-A55 | `-cpu cortex-a55 -smp 4` |
| GIC | GICv2 (default) | GICv3 (GIC-600) | `-M virt,gic-version=3` |
| RAM | 128 МиБ по умолчанию | 1 ГБ | `-m 1024M` |
| Адрес загрузки | ELF грузится по заголовкам | RAM с `0x0`, `tftp` + `go` на `0x0200_0000` | **две базы**: QEMU `0x42000000`, плата `0x02000000` ([[memory_map_rk3566]]) |
| UART | PL011 @ 0x09000000 | 16550-совместимый (DW), `reg-shift=2`, другой адрес | trait `Uart` + `Pl011`/`Dw16550` |
| Ethernet | virtio-net / e1000 (PCI) | GMAC (DesignWare) + RTL8211F; 2-й порт PCIe RTL8111H | стек в QEMU, драйвер GMAC на плате |
| CRU/GRF/DMA | нет | есть | только на плате |

Итоговый runner:

```bash
qemu-system-aarch64 -M virt,gic-version=3 -cpu cortex-a55 -smp 4 -m 1024M \
    -nographic -serial mon:stdio -kernel <elf>
```

> [!warning] Подводные камни
> - `-cpu max` включает фичи, которых нет на A55 (например, SVE) — код соберётся в QEMU и упадёт на плате. Если QEMU не знает `cortex-a55`, брать `cortex-a53`.
> - GICv3 ≠ GICv2 по программной модели: у v3 системные регистры `ICC_*_EL1` и редистрибьюторы вместо GICC.
> - `0x40000000` на плате с 1 ГБ — граница RAM, грузиться туда нельзя. Более того, проверено на железе: RAM начинается с `0x0`, поэтому `0x42000000` лежит **вне** RAM и для платы не годится ([[memory_map_rk3566]]).
> - U-Boot `go` передаёт в `x0` argc, а не DTB — на `x0` при старте не полагаться.

## Как выйти из QEMU (`cargo fwr`)

`-nographic -serial mon:stdio` — это мультиплексор: терминал в raw-режиме, эскейп QEMU = `Ctrl-A`.

| Клавиши | Действие |
|---|---|
| `Ctrl-A`, **отпустить**, затем `x` | выйти из QEMU (строчная `x`, **без** Ctrl) |
| `Ctrl-A`, затем `h` | справка по эскейпам |
| `Ctrl-A`, затем `c` | в монитор QEMU; назад — снова `Ctrl-A c` |
| в мониторе: `quit` | тоже выход |

> [!warning] Три ловушки
> - `Ctrl-A Ctrl-X` — это команда **picocom**, а не QEMU. QEMU после `Ctrl-A` ждёт одиночный символ; `Ctrl-X` даёт `0x18` и ничему не соответствует.
> - `Ctrl-C` не поможет: в raw-режиме он уходит гостю, а не QEMU (поэтому в `just smoke` и стоит `timeout`).
> - Если `Ctrl-A h` не показывает справку — эскейп перехватывает терминал/IDE. Запусти в обычном терминале либо смени эскейп: `-echr 0x1d` (тогда `Ctrl-]`). Вариант без монитора: `-serial stdio` вместо `-serial mon:stdio` — тогда `Ctrl-C` завершает QEMU.
>
> Аварийный выход: `pkill -f qemu-system-aarch64` из другого терминала.

## Репетиция U-Boot в QEMU

Собрать U-Boot с `qemu_arm64_defconfig`, запустить `-bios u-boot.bin`, TFTP поднять на хосте через `-netdev user,tftp=...`. Отрабатывается реальный handoff: уровень исключений, состояние регистров, семантика `go`.

## Граница QEMU/плата

Логика (сетевой стек, firewall, модули) и её тесты — в QEMU. Драйверы периферии и всё, что связано с таймингами, DMA, PHY, клоками — на плате короткими smoke-тестами.

> [!question] Для самопроверки
> - Почему `-cpu max` опасен для портируемости на реальное железо?
> - Чем программная модель GICv3 отличается от GICv2?
> - Почему адрес загрузки должен совпадать для QEMU и U-Boot?

## Источники
- QEMU docs: Arm virt machine (default RAM 128 MiB, `gic-version`)
- TRM RK3566: GIC-600 (GICv3), UART DesignWare 16550 (`reg-shift=2`, `reg-io-width=4`)
- U-Boot: `qemu_arm64_defconfig`, команда `go`

## Связанные заметки
- [[load_address]], [[board_abstraction]]
