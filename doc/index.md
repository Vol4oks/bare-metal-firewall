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
- [[asm_lsp_config]] — asm-lsp против компилятора: ложные ошибки x86-дефолта, фикс через .asm-lsp.toml.

## Hardware
- [[uart_pl011]] — регистры DR/FR, механика write_byte, volatile.
- [[address_sources]] — база vs смещения, проверка через dumpdtb, путь к DTB.
- [[gic_v3]] — GICv3: GICD/GICR/CPU-интерфейс, INTID, группы, приоритеты, ack/EOI.
- [[arm_generic_timer]] — CNTV, CNTFRQ, level-triggered PPI 27, первый IRQ.
*Появятся: memory map RK3566, Ethernet.*

## Concepts
- [[boot_flow]] — путь от включения до main: роль start.s и linker.ld.
- [[multicore_path]] — одно ядро сейчас: инвариант, путь к SMP, правило для кода.
- [[mmio_basics]] — read/write_volatile, адресная арифметика, границы volatile.
- [[print_macros]] — гигиена макросов: $crate, concat!, format_args!.
- [[mmu_basics]] — перевод адресов L0–L3, регистры включения, план этапа 2.
- [[a64_descriptors]] — форматы дескрипторов, биты атрибутов, AF-ловушка.
- [[blocks_vs_pages]] — почему блоки до heap, таблица регионов как инвестиция в рост.
- [[global_alloc]] — договор GlobalAlloc, Layout, bump-аллокатор.
- [[raw_pointers]] — сырые указатели: операции, правила безопасности, null.
- [[a64_assembly_basics]] — минимум A64 для чтения start.s: регистры, режимы адресации, ловушки.
- [[exceptions]] — классы исключений, VBAR_EL1, структура векторной таблицы.
- [[esr_decode]] — декод ESR_EL1 на реальном fault: WnR, DFSC, связь с уровнем таблиц MMU.
- [[irq_entry_exit]] — вход/выход из IRQ: trampoline, сохранение GPR, eret.
- [[atomic_counter]] — `static AtomicU32` для счётчика тиков: почему не `static mut` и что значит `Relaxed`.
- [[memory_barriers]] — `DSB`/`DMB`/`ISB`: порядок MMIO, зачем барьер после записи в GIC и при включении MMU.
- [[spinlock]] — `SpinLock<T>`: `AtomicBool` + `UnsafeCell`, `Acquire`/`Release`, guard, IRQ-deadlock, `try_lock`.
- [[no_std_testing]] — тестирование no_std: selftest в QEMU и host-тесты чистой логики с `std::thread`.
- *Появятся: critical_section, lock ordering.*

## Decisions
- [[0001_output_first]] — ADR: сначала вывод, потом MMU.
- [[0002_global_uart_handle]] — ADR: UnsafeCell + unsafe impl Sync, инвариант до этапа 4.
- [[0003_exceptions_first]] — ADR: этап 3 = векторы до GIC, чтобы падения печатались.
- [[0004_baremetal_vs_linux]] — ADR: bare-metal vs готовая ОС — что отпадает, что появляется, почему выбран bare-metal.
- [[0005_board_gic_modules]] — ADR: адреса в `board.rs`, драйвер GICv3 в `gic.rs`, `irq.rs` — диспетчер.

## Stages
- [[stage3_interrupts]] — разбор этапа 3 по шагам: векторы, тест fault, GICv3, ARM-таймер, DoD.
- [[stage4_sync]] — этап 4: `SpinLock`, UART/heap под блокировкой, `try_lock` в панике, `IrqSafeSpinLock`.

## Review
- [[load_address]] — единая база 0x42000000 для QEMU и платы.
- [[bss_alignment]] — ALIGN(16) для .bss под stp-цикл обнуления.
- [[entry_hygiene]] — daifset и парковка ядер в _start.
- [[uart_poll_condition]] — опрос FR: `== 1` вместо маски бита TXFF.
- [[vector_table_31]] — ревью 3.1: опечатка в `.global`, `todo!()` в обработчике, буквы `nm` (t/T).
- [[gic_mapping_and_waker]] — ревью 3.3: GIC в MMU, биты GICR_WAKER (1/2), `IPRIORITYR` байтом, GICD_CTLR.
- [[irq_slot_unexpected]] — ревью 3.4: `ticks = 0` + `Unexpected exception!` — IRQ-слот ещё смотрит в `unexpected`.
- [[stage3_precommit]] — pre-commit ревью этапа 3: аппаратные нюансы (ISB/RWP/ICFGR1/EOImode), clippy, гигиена коммита.
- [[stage3_refactor_precommit]] — ревью рефакторинга board/gic/timer: тики работают, но board не подключён к mmu/uart, clippy identity_op.
- [[stage4_sync_precommit]] — ревью `SpinLock`/selftest: тесты проходят, но обычная сборка даёт dead_code, UART/heap ещё не переведены.
- [[stage4_sync_final]] — финальное ревью этапа 4: UART/heap под `SpinLock`, clippy чист; остался порядок `selftest: OK` до `heap: ok`.
- *Запланировано: [[board_abstraction]].*
