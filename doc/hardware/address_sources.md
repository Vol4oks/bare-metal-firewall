---
tags: [адреса, mmio, dtb, qemu, rk3566]
aliases: [откуда адреса UART, memory map, address sources]
stage: 2
---

# Откуда берутся адреса периферии

> [!abstract] Суть
> Два уровня адреса: база устройства (свойство платформы) и смещения регистров (свойство IP-блока). База PL011 в QEMU virt = 0x09000000 — константа карты памяти машины; на плате — из TRM RK3566 / dts. Правильный способ в проде — DTB; хардкод — осознанный bootstrap-этап.

## Два уровня в нашем коде

| Уровень | Где в коде | Источник истины |
|---|---|---|
| База UART | `UART0 = 0x0900_0000` | карта памяти платформы: QEMU `hw/arm/virt.c`; на плате TRM RK3566 / dts |
| Смещения | `+0x00` DR, `+0x18` FR | TRM PL011 (ARM DDI 0183), `hw/char/pl011.c` |

При переезде QEMU → RK3566 смещения PL011 не меняются, база — меняется (и меняется сам IP: 16550 со своим набором смещений и `reg-shift=2`, т.е. регистр №i на `base + i*4`) — [[qemu_adaptation]].

## Как проверить в QEMU

```bash
# DTB, который QEMU генерирует на лету:
qemu-system-aarch64 -M virt,... -machine dumpdtb=virt.dtb -nographic
dtc -I dtb -O dts virt.dtb | grep -A6 "uart@9000000"   # reg = 0x09000000
```
Монитор QEMU (Ctrl-A c): `info mtree` — карта памяти; формат строки `начало-конец (prio, тип): имя`. PL011 в virt: `0x09000000-0x09000fff` — регион на 4 КБ (страничное выравнивание), определены только регистровые смещения из TRM, обращения к «дырам» — UB.

## Как на плате RK3566
- TRM RK3566, глава UART: базовые адреса UARTn.
- dts rk356x: узлы `serial@...`, свойства `reg`, `reg-shift=<2>`, `reg-io-width=<4>`.
- Wiki FriendlyElec (NanoPi R3S): debug-консоль = UART2, 1500000 бод.

> [!warning] Подводные камни
> - Хардкод базы — норм bootstrap; в проде адрес читают из DTB (QEMU кладёт его в RAM; при `-kernel` указатель обычно в `x0`, но `go` U-Boot перезапишет `x0` argc'ом — [[qemu_adaptation]]).
> - `reg-shift=2` у Rockchip: не забыть в арифметике при порте 16550-драйвера ([[board_abstraction]]).

## Связанные заметки
- [[uart_pl011]], [[qemu_adaptation]], [[mmio_basics]], [[board_abstraction]]
