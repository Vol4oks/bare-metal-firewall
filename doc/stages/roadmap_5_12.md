---
tags: [план, этап-5, workspace, роадмап, этап-12]
aliases: [roadmap, план 5-12, поэтапный план]
stage: 5
---

# План 5 → 12: от планировщика до управления

> [!abstract] Суть
> Приняты варианты **1A / 2B / 3B / 4A**: сначала этап 5 доводится до рабочего состояния в одном крейте, затем — миграция в workspace; сборка через алиасы без глобального `[build] target`; preemptive-переключение — опциональное упражнение после событийного планировщика; management — VLAN на LAN-порту. План ниже — источник истины по порядку работ; решения «почему так» — в [[0008_workspace_layout]], [[0009_hybrid_scheduling]], [[0010_management_http]], [[0011_update_and_slots]].

## Принятые варианты

| # | Вопрос | Решение | Следствие |
|---|---|---|---|
| 1 | Порядок | **A. Сначала этап 5 в одном крейте, потом workspace** | не смешиваем рефакторинг сборки с изучением планировщика |
| 2 | Host-тесты | **B. Без глобального `[build] target`, алиасы `cargo fw/fwr/fwt/fwc`** | `cargo build`/`cargo run` без алиаса больше не собирают прошивку (см. [[cargo_aliases]]) |
| 3 | Preemptive | **B. Опционально, после рабочего event-планировщика** | до 5.5 вытеснения нет: событие будит, но переключение — в кооперативной точке |
| 4 | Management | **A. VLAN на LAN-порту** | второй физический порт остаётся WAN |

Главное следствие 3B, которое надо держать в голове: **таймер не вытесняет задачу**. Задача, которая ждёт дедлайн, просыпается по таймеру, но получит CPU только когда текущая задача дойдёт до safe point (`wait`, `yield`, конец бюджета) или когда CPU уйдёт в idle. Для DP это нормально (бюджет — это и есть safe point), для CP — риск зависшей задачи. Это осознанная цена: сначала отлаживаем TCB и события, потом (если захотим) добавляем вытеснение.

## Фаза 0. Этап 5 в одном крейте

**Статус на 2026-11-05:** шаг 0.1 закрыт — все группы `sched: … ok`, `clippy -D warnings` и `fmt --check` чисты, `todo!()` не осталось. Следующий шаг — 0.2.

### 0.1 Компилируемый каркас политики (без asm)

Детальный разбор типов, семантики событий и selftest — [[stage5_policy_skeleton]]; план тестирования (уровни, матрица, randomized) — [[stage5_policy_testing]].

**Статус: закрыт (2026-11-05).**

- `TaskState`: `Unused | Ready | Running | Blocked | Exited`.
- `Task`: `ctx`, `state`, `prio`, `wait_mask`, `pending`, `stack`, `name`.
- `Scheduler`: `tasks: [Task; MAX_TASKS]`, `current`, `cursor`; **без аллокаций**.
- В `main` пока только `init` + idle-задача, поведение не меняется.

**DoD:** `cargo build` зелёный, `cargo clippy --bin bare-metal-firewall` чист, вывод в QEMU прежний.

### 0.2 Cooperative switch (5.1)

- `TaskContext` (`x19–x29`, `x30`, `sp`), `sched_switch` на asm ([[context_switch]]).
- Два статических стека с `#[repr(align(16))]`, канарейка в конце.
- `spawn`, `yield_now`, `start`; `main` уходит в `start()` и не возвращается.

Детали, ловушки edition 2024 (`static mut`) и раскладка asm — [[stage5_0_2_switch]].

**Статус: закрыт (2026-11-06).** Прогон: `switch ok`, `canary ok`; clippy чист во всех четырёх конфигурациях; негативный тест ловит отсутствие переключений. Попутно закрыт trap `EC=0x7` ([[fp_simd_in_kernel]]).

**DoD:** `A B A B …` без таймера; канарейки целы.

### 0.3 Idle и exit (5.2)

Детали — [[stage5_0_3_idle]].

- Idle-задача: `loop { wfi }`, но **после** `wfi` перепроверять `READY` (см. предупреждение в [[0009_hybrid_scheduling]] про `wfi` под маской).
- `exit_current` → `Exited`, выбор следующей; пустая Ready-очередь → idle.

**DoD:** задачи завершаются, тики продолжают идти, CPU не крутится вхолостую.

### 0.4 События (5.3) — ядро гибрида

Две атомарные переменные, которые трогает ISR, и ничего больше:

```rust
static READY: AtomicU32 = AtomicU32::new(0);          // бит i = задача i готова
static PENDING: [AtomicU32; MAX_TASKS] = [const { AtomicU32::new(0) }; MAX_TASKS];
static NEED_RESCHED: AtomicBool = AtomicBool::new(false);
```

- `wake_from_isr(id, bits)`: `PENDING[id].fetch_or(bits); READY.fetch_or(1 << id);`
- `wait(mask)`: сначала проверить `PENDING & mask` (событие могло прийти **до** `wait`); если пусто — `Blocked` + `wait_mask = mask`, затем перепроверить под короткой маской IRQ (закрыть гонку «проверил → заснул → событие пришло»).
- `pick_next()`: забрать бит из `READY`, прочитать `PENDING`, решить, что запускать.
- **Источник события на этом шаге — таймер**: `timer::on_irq` делает `wake_from_isr(id, TICK)` вместо печати/переключения. NIC появится на этапе 6 и займёт то же место.

**DoD:**
- задача спит на `TICK`, просыпается и печатает;
- событие, отправленное до `wait`, не теряется (проверяется сценарием «wake → wait»);
- ISR не печатает и не аллоцирует;
- idle-цикл уходит в `wfi` и выходит из него по событию.

### 0.5 Дедлайны вместо периодического тика (5.4)

- Заменить `CNTV_TVAL = freq/4` на одноразовый `CNTV_CVAL_EL0` по **ближайшему** дедлайну.
- Очередь дедлайнов — фиксированный отсортированный массив (до 8 записей), без аллокаций.
- «Печать тиков раз в 250 мс» становится обычным дедлайном CP-задачи, а не свойством таймера.

**DoD:** при пустой очереди прерываний нет; при двух дедлайнах (100 мс и 1 с) оба срабатывают в правильном порядке; ARP-подобная задача-пустышка живёт на 1 с.

### 0.6 Preemptive — отложено (5.5, опционально)

Не входит в DoD этапа 5. Если возвращаться: подмена IRQ-кадра на выходе из trampoline, `NEED_RESCHED`, квант только для CP-задач. DP не вытесняется никогда ([[0006_data_plane_async]]).

**DoD этапа 5 в целом:** 0.1–0.5 закрыты, `cargo fmt`/`clippy` чисты, README обновлён, `doc/stages/stage5_scheduler.md` синхронизирован с фактическим порядком.

## Фаза 1. Миграция в workspace

### 1.1 Механический перенос

```bash
mkdir -p firmware crates
git mv src firmware/src
git mv Cargo.toml firmware/Cargo.toml   # затем почистить
```

Корневой `Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["firmware", "crates/*"]
default-members = ["crates/fw-sync"]     # список растёт вместе с крейтами

[workspace.package]
edition = "2024"
version = "0.1.0"

[workspace.dependencies]
fw-sync = { path = "crates/fw-sync" }

# профили — ТОЛЬКО здесь, иначе panic = "abort" игнорируется
[profile.dev]
panic = "abort"
[profile.release]
panic = "abort"
```

`default-members` из host-тестируемых крейтов делает безопасными «голые» `cargo build`/`cargo test`: прошивка собирается только через алиасы.

### 1.2 Сборка через алиасы (вариант 2B)

`.cargo/config.toml` — убрать `[build] target` и `rustflags`, добавить:

```toml
[target.aarch64-unknown-none]
linker = "rust-lld"
runner = "qemu-system-aarch64 -M virt,gic-version=3 -cpu cortex-a55 -smp 4 -m 1024M -nographic -serial mon:stdio -kernel"

[alias]
fw  = "build  --target aarch64-unknown-none -p firmware"
fwr = "run    --target aarch64-unknown-none -p firmware"
fwc = "clippy --target aarch64-unknown-none -p firmware -- -D warnings"
fwt = "test -p fw-sync"          # список пакетов расширяется по мере появления крейтов
```

`firmware/build.rs` — единственное место, где живёт `-T`:

```rust
fn main() {
    println!("cargo::rerun-if-changed=../linker.ld");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("none") {
        let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo::rustc-link-arg=-T{root}/../linker.ld");
    }
}
```

**DoD:** `cargo fw` и `cargo fwr` дают прежний вывод; `cargo fwt` (пока пустой набор тестов) проходит; `cargo build` в корне не пытается собирать прошивку. Детали и грабли — [[cargo_aliases]].

### 1.3 Извлечение крейтов (строго по одному)

| Шаг | Крейт | Что уезжает | Тест |
|---|---|---|---|
| 1.3.1 | `fw-sync` | `SpinLock` | 4 потока × 10 000, `try_lock` ([[no_std_testing]]) |
| 1.3.2 | `fw-arch-a64` | `start.s`, `irq_vectors.s`, `sched_switch`, обёртки sysreg | QEMU: тот же вывод |
| 1.3.3 | `fw-bsp-qemu` | `board.rs` | QEMU |
| 1.3.4 | `fw-sched` | `Task`, `Scheduler`, `READY`/`PENDING`, дедлайны | host, таблица ниже |
| 1.3.5 | `fw-heap` | bump как тип; `#[global_allocator]` остаётся в `firmware` | host: `Layout`/границы |

Извлечение `IrqSafeSpinLock` требует маски DAIF, то есть asm. Поэтому: `fw-sync` остаётся чистым (`SpinLock`), а `IrqSafeSpinLock` живёт в `fw-arch-a64` (или в `firmware`) — иначе крейт перестаёт тестироваться на хосте.

**Тесты `fw-sched` на хосте (без asm и без MMIO):**

| Сценарий | Ожидание |
|---|---|
| round-robin двух Ready-задач | строгое чередование |
| приоритет: DP выше CP | CP получает CPU только когда DP пуст |
| `wake` до `wait` | `wait` возвращается сразу, событие не потеряно |
| `wake` после `wait` | задача становится Ready |
| бюджет DP исчерпан | управление возвращается планировщику |
| два дедлайна 100 мс / 1 с | порядок срабатывания |
| пустая Ready-очередь | выбирается idle |
| `MAX_TASKS` исчерпан | `spawn` возвращает `None`, без паники |

**DoD фазы 1:** `cargo fw` = прежнее поведение; `cargo fwt` зелёный; `cargo fwc` чист; ни один policy-крейт не зависит от MMIO/asm.

## Фаза 2. Этапы 6–9

| Этап | Крейт | Ключевое | Тесты |
|---|---|---|---|
| 6 | `fw-net` (eth) | замена «таймер-заглушки» на NIC-IRQ; NAPI-бюджет; таймстамп до ring | QEMU: кадры; плата: счётчик ISR |
| 7 | `fw-net` (arp/ip/icmp) | разбор/сборка заголовков отдельно от MMIO | host: golden-дампы пакетов |
| 8 | `fw-net` + `smoltcp` | проверить `no_std`-фичи и объём RAM | host: парсинг; QEMU: ping |
| 8.5 | `fw-modules` | трейты + реестр фиксированного размера (без аллокаций) | host: порядок вызова хуков |
| 9 | `fw-firewall` | правила, conntrack, NAT — чистая логика | host: first-match, состояния, NAT |
| 9 | `fw-config` | модель, `schema_version`, валидация | host: round-trip, миграции, отказ невалидного |

Правило этапа 6: NIC-IRQ встаёт ровно на место таймера-заглушки из 0.4 — `wake_from_isr(dp_task, NET_RX)`. Меняется один вызов, а не модель.

## Фаза 3. Этапы 10–12

| Этап | Что | Детали |
|---|---|---|
| 10 | U-Boot: `bootcount`/`altbootcmd`, выбор слота, TFTP + USB | [[0011_update_and_slots]] |
| 11 | `fw-mgmt`: HTTP/JSON, Digest, лимиты, статика | [[0010_management_http]]; management — VLAN на LAN |
| 12 | браузерная загрузка, SD-драйвер, staging, авто-откат | самая дорогая часть, только после 10–11 |

## Порядок и контрольные точки

```mermaid
flowchart LR
    S5["5.1 cooperative"] --> S52["5.2 idle/exit"]
    S52 --> S53["5.3 events"]
    S53 --> S54["5.4 deadlines"]
    S54 --> CP1{{"CT1: модель работает на таймере"}}
    CP1 --> WS["1.1-1.2 workspace"]
    WS --> CP2{{"CT2: cargo fw = прежний вывод"}}
    CP2 --> EXT["1.3 извлечение крейтов"]
    EXT --> CP3{{"CT3: cargo fwt зелёный"}}
    CP3 --> NET["6-8.5 сеть"]
    NET --> FW["9 файрвол + конфиг"]
    FW --> MG["11-12 mgmt + update"]
```

| Точка | Критерий прохода | Что делать, если не прошли |
|---|---|---|
| CT1 | событие будит задачу, гонка `wake`/`wait` закрыта, idle спит | не начинать миграцию: баг спрячется в манифестах |
| CT2 | `cargo fwr` — байт-в-байт прежнее поведение | откатить перенос, разбирать `-T`/профили |
| CT3 | host-тесты `fw-sched` зелёные, clippy чист | не тащить `fw-net` на непроверенной policy |

## Риски

> [!warning]
> - **Вариант 2B:** «голые» `cargo build`/`cargo run`/`cargo clippy` больше не работают для прошивки. Это цена нативного `cargo test`. Все команды — из [[cargo_aliases]]; приучить себя с первого дня.
> - **Миграция посреди этапа 5** превращает отладку asm в отладку манифестов. Поэтому CT1 обязателен.
> - **`#[panic_handler]`/`#[global_allocator]`/`_start`** должны остаться только в `firmware`; иначе host-тесты не соберутся.
> - **`IrqSafeSpinLock` в `fw-sync`** убьёт host-тесты (asm). Держать его в arch.
> - **Событие до `wait`**: без `PENDING`-маски оно теряется — самый частый баг event-планировщиков.
> - **Сброс `READY`** должен быть атомарным (`fetch_and(!bit)`), иначе два «потребителя» (IRQ и планировщик) разойдутся.

## Связанные заметки
- [[0008_workspace_layout]], [[0009_hybrid_scheduling]], [[0010_management_http]], [[0011_update_and_slots]], [[cargo_aliases]], [[stage5_scheduler]], [[stage5_tdd]], [[task_and_scheduler]], [[context_switch]], [[no_std_testing]]
