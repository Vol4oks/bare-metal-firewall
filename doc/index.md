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
- [[cargo_aliases]] — сборка без глобального `[build] target`: `cargo fw/fwr/fwc/fwt`, host-тесты, `-T` через build.rs.
- [[justfile]] — `just check/smoke/ci`: линт во всех конфигурациях, QEMU-прогоны с проверкой маркеров, прямой запуск с `-serial file:`.

## Hardware
- [[uart_pl011]] — регистры DR/FR, механика write_byte, volatile.
- [[address_sources]] — база vs смещения, проверка через dumpdtb, путь к DTB.
- [[gic_v3]] — GICv3: GICD/GICR/CPU-интерфейс, INTID, группы, приоритеты, ack/EOI.
- [[arm_generic_timer]] — CNTV, CNTFRQ, level-triggered PPI 27, первый IRQ.
- [[ethernet_queues_ptp]] — очереди GMAC/RTL8111H и аппаратные PTP-метки: что подтверждено, что нет, как проверять.
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
- [[task_and_scheduler]] — модель задач, состояния, round-robin, idle, cooperative vs preemptive.
- [[context_switch]] — сохранение контекста, cooperative `sched_switch`, preemptive IRQ-кадр.
- [[fp_simd_in_kernel]] — `CPACR_EL1.FPEN`, trap `EC=0x7`, почему компилятор всё равно использует NEON и что сохранять в контексте.
- [[latency_measurement]] — разрешение vs джиттер, где брать таймстамп (L0/L1/L2), polling vs IRQ, методика замера.
- [[firmware_update_patterns]] — как обновляют прошивки в сетевых устройствах: single bank/A/B/golden, bootcount и откат, подпись, TR-069/ACS, ISSU.
- [[multicore_path]] — одно ядро сейчас, per-core ресурсы, путь к SMP.
- *Появятся: critical_section, lock ordering.*

## Decisions
- [[0001_output_first]] — ADR: сначала вывод, потом MMU.
- [[0002_global_uart_handle]] — ADR: UnsafeCell + unsafe impl Sync, инвариант до этапа 4.
- [[0003_exceptions_first]] — ADR: этап 3 = векторы до GIC, чтобы падения печатались.
- [[0004_baremetal_vs_linux]] — ADR: bare-metal vs готовая ОС — что отпадает, что появляется, почему выбран bare-metal.
- [[0005_board_gic_modules]] — ADR: адреса в `board.rs`, драйвер GICv3 в `gic.rs`, `irq.rs` — диспетчер.
- [[0006_data_plane_async]] — ADR (предложен): data plane run-to-completion, control plane — executor; таймстамп до очередей.
- [[0007_core_partitioning]] — ADR: выделение ядер, per-core GIC/MMU/VBAR, PSCI, «IRQ-ядро» как антипаттерн.
- [[0008_workspace_layout]] — ADR: разбиение на крейты по границе «политика/механизм/железо», host-тесты, грабли Cargo.
- [[0009_hybrid_scheduling]] — ADR: гибридный планировщик — события NIC + дедлайны таймера, safe points, NAPI.
- [[0010_management_http]] — ADR: веб-интерфейс и management plane: HTTP/JSON, аутентификация, изоляция от WAN.
- [[0011_update_and_slots]] — ADR: обновление через U-Boot и браузер, A/B-слоты, откат, хранение конфига.
- [[0012_protection_module_loading]] — ADR (предложен): как подключать новые алгоритмы защиты — конфиг/правила/VM/нативный блоб, таблица плюсов и минусов.

## Stages
- [[stage3_interrupts]] — разбор этапа 3 по шагам: векторы, тест fault, GICv3, ARM-таймер, DoD.
- [[stage4_sync]] — этап 4: `SpinLock`, UART/heap под блокировкой, `try_lock` в панике, `IrqSafeSpinLock`.
- [[stage5_scheduler]] — этап 5: задачи, стеки, cooperative `yield`, preemptive таймер, idle.
- [[stage5_tdd]] — тесты до реализации: policy на хосте, mechanism в QEMU, порядок TDD.
- [[stage5_policy_skeleton]] — шаг 0.1: TCB, состояния, приоритеты, `pending`/`wait_mask`, selftest без asm.
- [[stage5_policy_testing]] — тестирование 0.1: compile-time asserts, матрица состояний, сценарии, randomized-прогон, перенос на host.
- [[stage5_0_2_switch]] — шаг 0.2: контекст новой задачи, статические стеки, `sched_switch`/`sched_start`, проводка в `main`.
- [[stage5_schedtest]] — организация `schedtest`: стеки и канарейки, задачи, монитор, контракт вывода, чекпоинты.
- [[stage5_0_3_idle]] — шаг 0.3: idle как задача, планировщик как основной цикл, `exit → idle`, тики из idle.
- [[stage5_0_4_events]] — шаг 0.4: `PENDING`/`READY`, реестр `WAKERS`, `wake_from_isr`, drain, `wait_current`, счётчик тиков.
- [[roadmap_5_12]] — план 5→12 по принятым вариантам 1A/2B/3B/4A: этапы, крейты, тесты, контрольные точки.

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
- [[stage5_schedtest_review]] — ревью `schedtest` (0.2 в работе): структура верна, 23 ошибки сборки, `default`-фичи ломают обычный запуск.
- [[stage5_0_2_precommit]] — pre-commit 0.2: все режимы работают, негативный тест ловит отсутствие переключений; блокер — clippy.
- [[stage5_0_3_review]] — ревью 0.3: работает, но clippy красный (`unreachable statement`, `Stack.0`), пропала `schedtest: start`, нет негативного теста idle.
- [[stage5_0_4_review]] — ревью 0.4: тики идут `(+1)`; блокер — `Stack.0` в `ticker.rs`, опечатка в `schedtest: star`, не прогнаны негативные сценарии.
- *Запланировано: [[board_abstraction]].*
