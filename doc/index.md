---
tags: [moc]
---

# Карта знаний (MOC)

> [!abstract] Суть
> Индекс всех заметок базы знаний. Пополняется по мере появления материалов.

## Toolchain
- [[qemu_adaptation]] — настройка QEMU virt под RK3566: CPU/GIC/RAM, абстракция платы, репетиция U-Boot.
- [[linker_script]] — зачем нужен linker.ld: секции, счётчик адреса, контракт с start.s.
- [[uboot_loading]] — как U-Boot грузит образ: go vs bootelf, роль адреса tftp.

## Hardware
*Появятся: memory map RK3566, UART, GIC, Ethernet.*

## Concepts
- [[boot_flow]] — путь от включения до main: роль start.s и linker.ld.
*Появятся: MMU, исключения, синхронизация.*

## Review
- [[load_address]] — единая база 0x42000000 для QEMU и платы.
- [[bss_alignment]] — ALIGN(16) для .bss под stp-цикл обнуления.
- [[entry_hygiene]] — daifset и парковка ядер в _start.
- *Запланировано: [[board_abstraction]].*
