---
tags: [review, этап-3, commit, качество]
aliases: [precommit review stage 3]
stage: 3
---

# Pre-commit ревью этапа 3

> [!abstract] Суть
> Функционально этап 3 закрыт: IRQ приходят, `ticks` растут. Для QEMU блокеров нет. Перед коммитом: не тащить `virt.dtb`, добавить новые `src/irq.rs`/`src/irq_vectors.s`, поправить аппаратные нюансы (ISB, RWP, ICFGR1, EOImode) и убрать тестовые артефакты.

## Проверено

- `cargo build` — без предупреждений.
- `cargo build --release` — без предупреждений.
- `cargo fmt -- --check` — чисто.
- `cargo clippy` — 7 warnings + ошибка `eq_op` (deny-by-default); причины и варианты — ниже.
- `.vectors` в debug: Addr `0x42004800`, Size `0x8c4`, ALIGN `2048`; `irq_vectors` — глобальный `T`.
- `.vectors` в release: Addr `0x42002000`, ALIGN `2048`; `irq_handler_asm` на месте. Риск из [[vector_table_31]] не проявился.
- GDB: IRQ приходит в слот `VBAR + 0x280`; FIQ-слот не срабатывает.

## Блокеры коммита

- `virt.dtb` (1 МБ) — untracked. Не коммитить: это дамп QEMU, а не исходник. Добавить в `.gitignore` или удалить.
- `src/irq.rs` и `src/irq_vectors.s` — untracked; не забыть `git add`.
- `doc/**` — решить, включать в этот же коммит или делать отдельный.

## Аппаратные нюансы (QEMU не ловит)

- `isb` после `msr vbar_el1`: новое значение должно вступить в силу до следующего исключения.
- `isb` после `msr daifclr, #2`: изменение маски — до следующей инструкции.
- `isb` после записи `CNTV_TVAL/CNTV_CTL` в `start_timer` и после перезарядки `TVAL` в обработчике (перед EOI).
- Дождаться `GICD_CTLR.RWP (бит 31) == 0` после включения `ARE/EnableGrp1`.
- `GICR_ICFGR1`: для PPI 27 выставить level — биты `[23:22] = 0`.
- `ICC_CTLR_EL1.EOImode`: проверить, что 0; иначе после `EOIR1` нужен ещё `ICC_DIR_EL1`.
- `CNTFRQ_EL0 != 0`: иначе `TVAL = 0` и шторм IRQ. На плате при нуле брать частоту из DTB.
- На плате: другие адреса GIC, проверить `CurrentEL` (EL1 vs EL2), per-core GICR для SMP.

## Статус после правок

| Пункт | Статус |
|---|---|
| `isb` после `msr vbar_el1` | ✅ |
| `isb` после `msr daifclr, #2` | ✅ |
| `isb` после `CNTV_TVAL/CNTV_CTL` в `start_timer` | ✅ |
| `isb` после перезарядки `TVAL` в обработчике | ✅ |
| `dsb sy` после записи `GICR_WAKER` | ✅ |
| `GICD_CTLR`: `ARE` + `EnableGrp1` | ✅ |
| `GICR_WAKER`: биты 1/2 | ✅ |
| `IPRIORITYR`: байтовая запись | ✅ |
| `ICC_SRE/PMR/IGRPEN1` + `isb` | ✅ |
| Ожидание `GICD_CTLR.RWP` | ⚠️ добавлено, но проверяется бит 12 вместо 31 |
| `GICR_ICFGR1`: PPI 27 level | ✅ |
| Проверка `ICC_CTLR_EL1.EOImode` | ⚠️ `msr` + `icc_ctrl_el1` → нужно `mrs` + `icc_ctlr_el1` |
| Проверка `CNTFRQ_EL0 != 0` | ✅ |
| Адреса платы / `CurrentEL` | ❌ (refactor + перед железом) |

## Повторная проверка после рефакторинга

- ❌ `EOImode`: `msr {}, icc_ctrl_el1` — две ошибки в одной строке: нужен `mrs` (чтение) и `icc_ctlr_el1` (Control Register).
- ❌ RWP: цикл проверяет `1 << 12`; правильный бит — 31 (`GICD_CTLR_RWP`). Бит 12 не RWP, условие всегда ложно — ожидания фактически нет.
- ⚠️ `GICD_CTLR` и `GICD_CTLR_RWP` объявлены, но не используются → два warning'а. После замены `GICD_BASE` и `1 << 12` на эти константы warnings исчезнут.
- ✅ `ICFGR1` level, порядок group→config→priority→enable, `CNTFRQ` check, `# Safety`, UART invariant, heap `checked_add`.

## Качество кода

- `sync_handler`: сообщение `panic!("Data abort test")` — тестовый артефакт. Заменить на `Unhandled synchronous exception` (EC/ISS уже печатаются).
- `init_gic`: безопасная `pub fn` с предусловиями (MMU включён, GIC отображён, вызов один раз). Сделать `unsafe fn` или добавить `# Safety`.
- `read_icc_iar1_el1`: чтение IAR имеет побочный эффект (ack). Сделать `unsafe fn` или зафиксировать инвариант в doc-комментарии.
- Магические числа GIC-офсетов (`0x14`, `0x80`, `0x100`, `0x400`) и битов — вынести в именованные константы.
- `cntfrq()` вызывается на каждом тике — закэшировать частоту при инициализации.
- Чтение WAKER через `*mut u32` — использовать `*const u32`.
- `uart::Uart`: комментарий «IRQ замаскированы в `_start`» устарел после `daifclr`. IRQ-обработчик не печатает, поэтому сейчас нет конкурентного доступа, но `sync_handler`/`unexpected` печатают из контекста исключения. До `SpinLock` (этап 4) зафиксировать инвариант в комментарии.
- `heap::Bump`: `aligned + layout.size()` без проверки переполнения; для 1 МБ heap не критично, но `checked_add` дешевле, чем отладка.

## Clippy

- **`eq_op` (`mmu.rs:72`) — ошибка**, deny-by-default: `(0 << 2)` и `(0b00 << 6)` — равные выражения под `|`. Это ложное срабатывание на документировании нулевых полей. Варианты: рефакторинг `attrs` (рекомендуется) или `#[allow(clippy::eq_op)]`.
- **`identity_op` (`mmu.rs:72–73`, `83–86`)** — `(0 << 2)`, `(0b00 << 6)`, `(0b00 << 14)` не меняют значение. Убрать нули и описать поля в комментарии или поставить `#[allow]`.
- **`deref_addrof` (`mmu.rs:45–47`)** — **не применять**: `&mut *(&raw mut L0_TABLE)` обходит `static_mut_refs` в edition 2024. Лучше `#[allow(clippy::deref_addrof)]` с комментарием.
- `manual_is_multiple_of`, `unnecessary_cast` — уже исправлены.
- `cargo clippy --all-targets` падает из-за отсутствия `test` crate у `no_std`; использовать `cargo clippy --bin bare-metal-firewall`.

Рекомендуемый рефакторинг:

```rust
fn attrs(kind: RegionType) -> u64 {
    // [1:0]=0b01 Block, [9:8] SH=00, [10] AF=1, [4:2] AttrIndx, [7:6] AP=00
    let base = (1 << 10) | 0b01;            // AF | Block
    match kind {
        RegionType::Normal => base,             // AttrIndx=0
        RegionType::Device => base | (1 << 2),  // AttrIndx=1
    }
}

let tcr_value: u64 = 16                     // T0SZ=16 → 48-бит VA
    | (0b01 << 8) | (0b01 << 10)            // IRGN0, ORGN0
    | (0b11 << 12);                         // SH0
// TG0 = 00 (4KB) — нулевое поле, сдвиг не нужен
```

## Рекомендуемая структура коммитов

```text
1. Этап 3: GICv3, таймер и первый IRQ
   src/irq.rs, src/irq_vectors.s, src/main.rs, src/mmu.rs, linker.ld

2. doc: база знаний этапа 3
   doc/**
```

`virt.dtb` не коммитить.

## Связанные заметки
- [[irq_slot_unexpected]], [[gic_mapping_and_waker]], [[vector_table_31]], [[irq_entry_exit]], [[memory_barriers]]
