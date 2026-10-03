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
- [[uart_pl011]] — регистры DR/FR, механика write_byte, volatile.
- [[address_sources]] — база vs смещения, проверка через dumpdtb, путь к DTB.
*Появятся: memory map RK3566, GIC, Ethernet.*

## Concepts
- [[boot_flow]] — путь от включения до main: роль start.s и linker.ld.
- [[multicore_path]] — одно ядро сейчас: инвариант, путь к SMP, правило для кода.
- [[mmio_basics]] — read/write_volatile, адресная арифметика, границы volatile.
- [[print_macros]] — гигиена макросов: $crate, concat!, format_args!.
*Появятся: MMU, исключения, синхронизация.*

## Decisions
- [[0001_output_first]] — ADR: сначала вывод, потом MMU.
- [[0002_global_uart_handle]] — ADR: UnsafeCell + unsafe impl Sync, инвариант до этапа 4.

## Review
- [[load_address]] — единая база 0x42000000 для QEMU и платы.
- [[bss_alignment]] — ALIGN(16) для .bss под stp-цикл обнуления.
- [[entry_hygiene]] — daifset и парковка ядер в _start.
- [[uart_poll_condition]] — опрос FR: `== 1` вместо маски бита TXFF.
- *Запланировано: [[board_abstraction]].*
