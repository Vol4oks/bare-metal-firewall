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
| Адрес загрузки | ELF грузится по заголовкам | `tftp 0x42000000` + `go` | линковать на `0x42000000` |
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
> - `0x40000000` на плате с 1 ГБ — граница RAM, грузиться туда нельзя; QEMU `-kernel` уважает ELF-заголовки, поэтому база `0x42000000` работает в обоих сценариях.
> - U-Boot `go` передаёт в `x0` argc, а не DTB — на `x0` при старте не полагаться.

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
