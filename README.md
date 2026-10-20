> [!NOTE]  
> Это учебный проект. В правилах ИИ задано, что я пишу код, а ИИ объясняет теорию и проводит ревью кода.  

# Bare-Metal Firewall on NanoPi R3S

Проект по разработке программного файрвола на Rust, работающего напрямую на железе (bare-metal) без операционной системы. Целевая платформа — **FriendlyElec NanoPi R3S LTS** (Rockchip RK3566, ARM Cortex-A55, 1 ГБ RAM, без eMMC).

---

## К чему идём

**Цель**: собственный файрвол на Rust, работающий на голом железе, с модульной архитектурой для обработки трафика.

**Ключевые характеристики финального продукта**:

- Работает без ОС — только ваш код и железо.
- Два нативных гигабитных порта (RTL8211F + RTL8111H через PCIe).
- Собственный сетевой стек или `smoltcp` (ARP, IP, ICMP, UDP, TCP).
- Stateful-файрвол с таблицей соединений и NAT.
- **Модульная система**: egress/ingress/connection-хуки для подключения логики обработки пакетов.
- Загрузка через U-Boot + TFTP, без перепрошивки носителя.

---

## Текущее состояние

**Актуально на 2026-10**:

- Есть плата **NanoPi R3S LTS** (1 ГБ RAM, без eMMC) и USB-UART адаптер CH340G (3.3V).
- Установлен Rust (edition 2024), цель `aarch64-unknown-none`, QEMU, `gdb-multiarch`.
- Каркас: `_start` (парковка ядер, daifset, стек, обнуление .bss), `linker.ld` (база 0x42000000, стек, heap), `panic_handler` с печатью, UART PL011-драйвер с `print!/println!`.
- **MMU включён**: identity map (RAM 1 ГБ Normal, GIC 2 МБ Device, UART 2 МБ Device) через таблицы L0–L2 с блоками, `TTBR0/TCR/MAIR/SCTLR`.
- **Heap работает**: bump-аллокатор как `#[global_allocator]`, `Box` выделяется в heap-регионе 1 МБ.
- **Этап 3 закрыт в QEMU**: GICv3 (GICD/GICR, Group 1, PPI 27), ARM Generic Timer `CNTV`, IRQ-векторы с trampoline, `ticks` растут ~4 раза в секунду.
- **Этап 4 закрыт**: `SpinLock` (`sync.rs`), UART и bump-аллокатор под блокировкой, `try_print_args` в panic/exception, selftest (`cargo run --features selftest`).
- **Модули этапов 3–4**: `board.rs` (адреса платы), `gic.rs` (GICv3), `timer.rs` (таймер и счётчик), `irq.rs` (VBAR и диспетчер), `irq_vectors.s` (векторы), `sync.rs` (SpinLock), `selftest/` (тесты).
- База знаний — в `doc/` (Obsidian, карта знаний `doc/index.md`).

**Что уже работает**:

```
$ cargo run
Hello from QEMU virt!
...
MMU on!
VBAR: 0x42005800
ticks = 0
ticks = 1
ticks = 2
...
```

Self-test:

```
$ cargo run --features selftest
...
selftest: start
  try_lock: ok
  data: ok
  Send + Sync: ok
  heap: ok
selftest: OK
ticks = 0
ticks = 1
...
```

Адрес `VBAR` зависит от сборки и размера `.text`; важно, чтобы он был кратен 2 КБ, а не конкретное значение.

**Что ещё не сделано**:

- Запуск на реальном железе (плата в наличии, отложено на этап 1/6). Перед этим: адреса GIC/UART из TRM/DTB, проверка `CurrentEL`, замена `board.rs`.
- Планировщик (этап 5).
- Драйвер Ethernet, сетевой стек, логика файрвола, модульная система, DPI-модуль.

---

## План разработки

Движение от «Hello World» до полноценного файрвола. Каждый этап опирается на предыдущий.

| Этап | Цель | Результат |
|:---:|---|---|
| **0** | Инструменты и окружение | Тулчейн, QEMU, UART, SD-карта |
| **1** | Первый запуск на железе | `Hello from Rust` в UART через U-Boot + TFTP |
| **2** | Управление памятью | MMU, таблицы страниц, аллокатор, `Box`/`Vec` |
| **3** | Прерывания и исключения | Векторы, data abort, GICv3, ARM-таймер, `ticks` |
| **4** | Синхронизация | `SpinLock`, UART/heap под блокировкой, selftest |
| **5** | Планировщик | Задачи, переключение контекста, round-robin |
| **6** | Драйвер Ethernet | TX/RX кадров через RTL8211F (нативный) |
| **7** | ARP, IP, ICMP | `ping` с другого компьютера |
| **8** | UDP/TCP | `smoltcp` или собственная реализация |
| **8.5** | Каркас модульной системы | Трейты `EgressModule`, `IngressModule`, `ConnectionModule`, реестр |
| **9** | Логика файрвола | Правила, conntrack, NAT, счётчики |
| **9.5** | Модуль обхода DPI | Стратегии split/fake/badseq, профили, хостлисты |
| **10** | Стабилизация | Профилирование, watchdog, тесты, документация |

---

## Архитектура проекта

```
bare-metal-firewall/
├── .cargo/
│   └── config.toml          # Целевая платформа, линкер, runner (QEMU virt, GICv3, A55×4)
├── src/
│   ├── main.rs              # Точка входа в Rust, инициализация и smoke-тесты
│   ├── start.s              # Ассемблерная точка входа _start (парковка ядер, bss)
│   ├── uart.rs              # UART PL011 под SpinLock, print!/println! (этап 0–1, 4)
│   ├── mmu.rs               # MMU: L0–L2, identity map RAM/GIC/UART (этап 2–3)
│   ├── heap.rs              # Bump под SpinLock, #[global_allocator] (этап 2, 4)
│   ├── board.rs             # Адреса платы: RAM, GICD, GICR, UART (этап 3)
│   ├── gic.rs               # GICv3: GICD/GICR, PPI, init/ack/eoi (этап 3)
│   ├── timer.rs             # ARM Generic Timer CNTV, ticks (этап 3)
│   ├── irq.rs               # VBAR, sync/IRQ dispatch (этап 3)
│   ├── irq_vectors.s        # 16 векторов + IRQ trampoline (этап 3)
│   ├── sync.rs              # SpinLock, guard, try_lock (этап 4)
│   ├── selftest/
│   │   ├── mod.rs           # запуск selftest по feature
│   │   ├── test_sync.rs     # try_lock/data/Send+Sync
│   │   └── test_heap.rs     # Box через GlobalAlloc
│   ├── sched.rs             # Планировщик (этап 5)
│   ├── net/
│   │   ├── eth.rs           # Драйвер Ethernet (этап 6)
│   │   ├── arp.rs           # ARP (этап 7)
│   │   ├── ip.rs            # IPv4 (этап 7)
│   │   └── icmp.rs          # ICMP (этап 7)
│   ├── firewall/
│   │   ├── rules.rs         # Правила (этап 9)
│   │   ├── conntrack.rs     # Таблица соединений (этап 9)
│   │   └── nat.rs           # NAT (этап 9)
│   └── modules/
│       ├── mod.rs           # Трейты модульной системы (этап 8.5)
│       └── dpi_bypass.rs    # Модуль обхода DPI (этап 9.5)
├── doc/                     # База знаний (Obsidian): index.md — карта
├── AGENTS.md                # Правила работы агента-наставника
├── linker.ld                # Скрипт компоновки (база 0x42000000, стек, heap 1 МБ)
├── Cargo.toml               # Манифест: panic = "abort", feature selftest
└── README.md
```

---

## Стек технологий

| Компонент | Выбор | Обоснование |
|---|---|---|
| Язык | Rust (`no_std`, `#![no_main]`) | Безопасность памяти, трейты для модулей |
| Целевая платформа | `aarch64-unknown-none` | Bare-metal ARM64 |
| Линкер | `rust-lld` | Встроенный, не требует внешнего |
| Эмулятор | QEMU `virt` (AArch64) | Быстрая отладка без железа |
| Прерывания | GICv3 + ARM Generic Timer (`CNTV`) | PPI 27, level-triggered, ~4 Гц в QEMU |
| Отладчик | `gdb-multiarch` | Пошаговая отладка, брейкпоинты |
| Загрузчик | U-Boot | Уже прошит, загрузка по TFTP |
| Сетевой стек | `smoltcp` (план) | Зрелый `no_std` TCP/IP стек на Rust |
| Синхронизация | свой `SpinLock` (этап 4) | UART/heap под блокировкой; `try_lock` в panic/exception |
| Тестирование | selftest в QEMU (`--features selftest`) | `try_lock`, heap, panic path; host-тесты — опционально |

---

## ⚙️ Ключевые решения

### Почему SD-карта, а не eMMC

Плата взята в конфигурации **без eMMC** — это осознанный выбор. Прошивка на eMMC отключает загрузку с SD-карты, что превращает восстановление после ошибок в сложную процедуру через Maskrom. SD-карта — единственный носитель, который можно перепрошить на компьютере за минуту.

### Почему `panic = "abort"`

В `no_std` нет `libunwind` и таблиц `.eh_frame`, поэтому разворачивание стека невозможно. Паника завершается вызовом `#[panic_handler]` и остановкой процессора.

### Почему `board.rs`, `gic.rs` и `timer.rs`

Адреса платы (RAM, GICD, GICR, UART) собраны в `board.rs`; регистры и последовательность GICv3 — в `gic.rs`; `CNTV` и счётчик тиков — в `timer.rs`; `irq.rs` остаётся диспетчером. При переносе на RK3566 меняется `board.rs`, а не три файла. Решение зафиксировано в `doc/decisions/0005_board_gic_modules.md`.

### Почему `SpinLock` и `try_print_args`

UART и heap — общее состояние, поэтому они под `SpinLock`. `Bump` владеет `SpinLock<BumpState>`, поэтому `Bump: Sync` получается автоматически, без `unsafe impl Sync`. В `panic_handler` и `unexpected` используется `try_print_args`: паника может случиться, когда UART-lock уже удержан, и `lock()` завис бы навсегда. Heap-lock нельзя ждать в панике/ISR — аллокатор из этих контекстов не вызывается.

### Почему модульная система на трейтах

Rust-трейты дают изоляцию модулей, возможность тестирования и динамической регистрации. Egress-хук — точка для модуля обхода DPI, ingress-хук — для фильтрации, connection-хук — для событий.

---

## Теоретическая база

Проект опирается на две книги:

- **OSTEP («Операционные системы: три простых элемента»)** — главы про процессы, память, concurrency, планировщик, I/O.
- **«Сетевое программирование от основ до приложений»** — главы про уровни, ARP, IP, ICMP, TCP/UDP, безопасность.

---

## Быстрый старт

### Сборка и запуск в QEMU

```bash
# Установка инструментов (Ubuntu/Debian)
sudo apt install qemu-system-arm gdb-multiarch build-essential lld
rustup target add aarch64-unknown-none

# Сборка и запуск
cargo run
```

Ожидаемый вывод:

```
Hello from QEMU virt!
...
MMU on!
VBAR: 0x42005800
ticks = 0
ticks = 1
...
```

`ticks` печатаются примерно 4 раза в секунду; конкретный адрес `VBAR` зависит от сборки.

### Self-test в QEMU

```bash
cargo run --features selftest
```

Ожидаемый вывод:

```
selftest: start
  try_lock: ok
  data: ok
  Send + Sync: ok
  heap: ok
selftest: OK
ticks = 0
ticks = 1
...
```

Self-test запускается до включения IRQ и проверяет `SpinLock`, `GlobalAlloc` и panic-path.

### Отладка через GDB

```bash
# Терминал 1: QEMU в режиме отладки
qemu-system-aarch64 -M virt,gic-version=3 -cpu cortex-a55 -smp 4 -m 1024M -nographic \
    -kernel target/aarch64-unknown-none/debug/bare-metal-firewall \
    -s -S

# Терминал 2: GDB
gdb-multiarch target/aarch64-unknown-none/debug/bare-metal-firewall
(gdb) target remote :1234
(gdb) break main
(gdb) continue
```

### Запуск на реальном железе (план)

```bash
# В UART-консоли (1500000 бод), прервать автозагрузку U-Boot
=> setenv ipaddr 192.168.1.10
=> setenv serverip 192.168.1.1
=> tftp 0x42000000 firewall.bin
=> go 0x42000000
```

---

## Текущий статус

- [x] Этап 0: инструменты и окружение
- [x] Этап 1 (QEMU): Hello World + UART-драйвер, печатающая паника
- [ ] Этап 1 (железо): запуск на NanoPi R3S — *плата в наличии, отложено (адрес 0x42000000 и сценарий U-Boot готовы, см. doc/review/load_address.md)*
- [x] Этап 2: MMU (identity map) и bump-аллокатор
- [x] Этап 3 (QEMU): векторы, data abort, GICv3, ARM-таймер, `ticks` — *на железе не проверялся*
- [x] Этап 4 (QEMU): `SpinLock`, UART/heap под блокировкой, selftest
- [ ] Этап 5: планировщик
- [ ] Этап 6: драйвер Ethernet
- [ ] Этап 7: ARP/IP/ICMP
- [ ] Этап 8: UDP/TCP
- [ ] Этап 8.5: модульная система
- [ ] Этап 9: логика файрвола
- [ ] Этап 9.5: модуль обхода DPI
- [ ] Этап 10: стабилизация
