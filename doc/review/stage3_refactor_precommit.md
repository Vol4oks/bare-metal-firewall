---
tags: [review, этап-3, рефакторинг, board, gic, commit]
aliases: [refactor review, precommit board gic timer]
stage: 3
---

# Ревью рефакторинга board/gic/timer перед коммитом

> [!abstract] Суть
> Рефакторинг собирается, `cargo run` печатает растущие `ticks`, release-таблица векторов на месте. Перед коммитом осталось подключить `board.rs` к `mmu.rs`/`uart.rs` (сейчас два warning'а), убрать clippy `identity_op` и добавить новые файлы в git.

## Проверено

- `cargo build` — 2 warning'а: `RAM_BASE`/`UART_BASE` не используются.
- `cargo build --release` — то же.
- `cargo fmt -- --check` — чисто.
- `cargo clippy --bin bare-metal-firewall -- -W clippy::all` — 3 warning'а: те же два + `identity_op` в `mmu.rs`.
- `cargo run` — `ticks` растут (17..39 за 10 с).
- `.vectors` release: `0x42002800`, align 2048, size `0x8c4`; `irq_vectors` — `T`.
- `git diff --check` — чисто.

## Находки

### Блокер перед коммитом

1. **`board.rs` не подключён к `mmu.rs` и `uart.rs`.**
   - `mmu.rs` всё ещё использует литералы `0x4000_0000`, `0x0800_0000`, `0x0900_0000`.
   - `uart.rs` всё ещё `const UART_ADDR = 0x0900_0000`.
   - Поэтому `board::RAM_BASE` и `board::UART_BASE` — dead code.
   - Фикс: `mmu.rs` → `board::RAM_BASE/GICD_BASE/UART_BASE`; `uart.rs` → `Pl011::new(board::UART_BASE)`.

### Важно

2. **Clippy `identity_op` в `mmu.rs:86`.**
   - `(0b00 << 14)` — нулевое поле `TG0`.
   - Либо убрать сдвиг и оставить комментарий `// TG0 = 00 (4KB)`, либо `#[allow(clippy::identity_op)]` с пояснением.
   - `attrs()` уже приведён в порядок, `deref_addrof` подавлен — хорошо.

3. **Гигиена коммита.**
   - Новые: `src/board.rs`, `src/gic.rs`, `src/timer.rs` — добавить.
   - Новые doc: `doc/concepts/spinlock.md`, `doc/stages/stage4_sync.md` — лучше отдельным коммитом.
   - `src/mmu.rs`, `src/uart.rs` появятся в diff после подключения `board`.

### Стиль

4. `timer::cntfrq()` — `pub`, хотя используется только внутри `timer.rs`; сузить до `fn` (если не нужен дебаг из main).
5. `irq::init()` — переменная `readbeck` (опечатка `readback`), косметика.
6. В `main` остались stage-2 отладочные принты (`bss size`, `stack gap`, heap addresses). Для учебного коммита ок; решить, оставлять ли.

## Что не проверялось

- Реальное железо: адреса `board.rs` — QEMU virt; RK3566 другие.
- SMP/per-core GICR: `gicr_for_core(0)` пока только ядро 0.
- Stage 4: UART/heap всё ещё на `UnsafeCell` + `unsafe impl Sync`; `doc/concepts/spinlock.md` и `doc/stages/stage4_sync.md` — план, не реализация.

## Рекомендуемые коммиты

```text
1. refactor: board.rs + gic.rs + timer.rs, mmu/uart на board
2. docs: база знаний этапа 3/4
```

`virt.dtb` в статусе отсутствует — хорошо.

## Связанные заметки
- [[stage3_precommit]], [[gic_v3]], [[arm_generic_timer]], [[0005_board_gic_modules]], [[spinlock]], [[stage4_sync]]
