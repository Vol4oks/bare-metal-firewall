---
tags: [workspace, cargo, миграция, тесты, этап-5]
aliases: [workspace migration, переезд в workspace, 1.1, 1.2]
stage: 5
---

# Миграция в workspace: пошаговый план

> [!abstract] Суть
> Переезд — механический: код не меняется, меняются только манифесты, пути и способ вызова cargo. Гейт один: после миграции `cargo fw` в QEMU даёт **тот же вывод**, что и до неё. Пока этот гейт не пройден, крейты не извлекаются, а этап 5 не продолжается.

## Прежде чем начинать

> [!warning] Порядок, зафиксированный в [[0008_workspace_layout]]
> Миграция планировалась **после** шага 0.5 (дедлайны), потому что отладка asm внутри миграции удваивает неизвестные. Сейчас 0.5 не закрыт, зато H0 закрыт и есть шанс, что на железе вылезут те же баги, что уже пойманы в QEMU.
>
> Практический компромисс: разрешено мигрировать сейчас, но **до H2/0.5** и строго с гейтом «поведение не изменилось». Если `just smoke` после миграции красный — откатываем миграцию, а не правим asm по ходу.

> [!success] Решено 2026-11-11
> - Префикс `fw-` у крейтов остаётся.
> - Порядок: сначала workspace (1.1–1.2), затем `fw-sync`, затем H2 и 0.5 уже в новом дереве.
> - `probe/` имеет собственный `[workspace]` и миграцию не замечает.
> - **1.1–1.2 пройдены:** `cargo fw`/`fwc` работают, QEMU-вывод совпал с эталоном (включая `VBAR`), оставались ошибки в `justfile` — см. [[justfile]].

## Шаг 1.1 — механический перенос

```bash
mkdir -p firmware crates
git mv src firmware/src
git mv Cargo.toml firmware/Cargo.toml
# linker.ld и .cargo/ остаются в корне
```

Корневой `Cargo.toml` (новый):

```toml
[workspace]
resolver = "3"
members = ["firmware", "crates/*"]

# пока host-крейтов нет — можно оставить строку пустой/убрать;
# после 1.3.1: default-members = ["crates/fw-sync"]
[workspace.package]
edition = "2024"
version = "0.1.0"

[workspace.dependencies]
# fw-sync = { path = "crates/fw-sync" }

# профили — ТОЛЬКО здесь: в манифесте члена Cargo их игнорирует
[profile.dev]
panic = "abort"
[profile.release]
panic = "abort"
```

`firmware/Cargo.toml` (из перенесённого, почистить):

```toml
[package]
name = "firmware"
version.workspace = true
edition.workspace = true

[features]
default = []
selftest = []
schedtest = []

[dependencies]
```

Что удалить из него: `[profile.dev]`/`[profile.release]` (теперь в корне), иначе `panic = "abort"` молча не применится.

`firmware/build.rs` (новый):

```rust
fn main() {
    println!("cargo::rerun-if-changed=../linker.ld");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("none") {
        let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo::rustc-link-arg=-T{root}/../linker.ld");
    }
}
```

`.cargo/config.toml` (правка):

```toml
[target.aarch64-unknown-none]
linker = "rust-lld"
runner = "qemu-system-aarch64 -M virt,gic-version=3 -cpu cortex-a55 -smp 4 -m 1024M -nographic -serial mon:stdio -kernel"

[alias]
fw  = "build  --target aarch64-unknown-none -p firmware"
fwr = "run    --target aarch64-unknown-none -p firmware"
fwc = "clippy --target aarch64-unknown-none -p firmware -- -D warnings"
```

> [!warning] `fwc` не принимает дополнительные флаги
> Всё, что дописывается к вызову алиаса, попадает в конец команды — то есть **после `--`**, к rustc. `cargo fwc --features selftest` даст ошибку `unexpected argument '--features'`.
> Для четырёх конфигураций clippy в `justfile` вызывать явно:
>
> ```make
> clippy:
>     cargo clippy --target aarch64-unknown-none -p firmware -- -D warnings
>     cargo clippy --target aarch64-unknown-none -p firmware --features selftest -- -D warnings
>     cargo clippy --target aarch64-unknown-none -p firmware --features schedtest -- -D warnings
>     cargo clippy --target aarch64-unknown-none -p firmware --features selftest,schedtest -- -D warnings
> ```

Убрать `[build] target` и `rustflags` с `-Tlinker.ld`: иначе host-тесты уедут в кросс-сборку, а относительный путь к линкеру сломается при запуске из подкаталога.

> [!warning] Цена варианта 2B
> «Голые» `cargo build`/`cargo run` больше не собирают прошивку. Пока нет ни одного host-крейта, `cargo build` из корня будет пытаться собрать `firmware` под host и падать — это ожидаемо, работаем только через `cargo fw`/`fwr`.

## Гейт CT2

```bash
cargo fw                       # собирается
cargo fwr                      # QEMU: прежний вывод
just smoke                     # должно быть зелёно (после правки justfile)
```

Обязательные правки `justfile`:

- `kernel := "target/aarch64-unknown-none/debug/bare-metal-firewall"` → `.../debug/firmware`;
- `cargo build` → `cargo fw`, `cargo build --release` → `cargo fw --release`, `cargo build --features selftest` → `cargo fw --features selftest`;
- `cargo run` / `cargo run --features …` → `cargo fwr` / `cargo fwr --features …`;
- блок `clippy` — явные вызовы с `--target` и `-p firmware` (см. предупреждение выше), потому что `cargo fwc --features …` не работает;
- `symbols`: путь `nm` к `…/firmware`;
- `cargo fmt` не меняется.

Без этого `just smoke` будет искать несуществующий бинарь и «падать» на пустом месте.

## Шаг 1.2 — профили, линт, форма

Профили только в корне — проверка:

```bash
cargo fw 2>&1 | grep -i "profiles for the non root"   # не должно быть
```

## Шаг 1.3 — извлечение крейтов (по одному)

| Шаг | Крейт | Что уезжает | Гейт |
|---|---|---|---|
| 1.3.1 | `fw-sync` | `SpinLock` | `cargo fwt` на хосте |
| 1.3.2 | `fw-arch-a64` | `start.s`, `irq_vectors.s`, `sched_switch`, sysreg | `just smoke` |
| 1.3.3 | `fw-bsp-qemu` | `board.rs` | `just smoke` |
| 1.3.4 | `fw-sched` | `Task`, `Scheduler`, `PENDING`/`READY` | host-тесты + `just smoke` |
| 1.3.5 | `fw-heap` | bump как тип (`#[global_allocator]` остаётся в `firmware`) | host-тесты границ |

Правило: после каждого шага `cargo fwc` и `just smoke` зелёные, иначе шаг откатывается.

### `fw-sync`: как должен выглядеть крейт

```rust
// crates/fw-sync/src/lib.rs
#![cfg_attr(not(test), no_std)]

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
// Send для SpinLock<T> выводится автоматически при T: Send
```

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn four_threads_ten_thousand() {
        let lock = Arc::new(SpinLock::new(0u32));
        let mut hs = Vec::new();
        for _ in 0..4 {
            let l = Arc::clone(&lock);
            hs.push(std::thread::spawn(move || {
                for _ in 0..10_000 {
                    *l.lock() += 1;
                }
            }));
        }
        for h in hs {
            h.join().unwrap();
        }
        assert_eq!(*lock.lock(), 40_000);
    }
}
```

`fwt` в алиасах появляется одновременно с крейтом — пакет, которого нет в графе, ломает алиас.

## Где какие тесты

| Уровень | Что проверяет | Чем запускается |
|---|---|---|
| host | policy: `SpinLock`, планировщик, правила, конфиг | `cargo fwt` (нативно, без QEMU) |
| QEMU | asm, MMU, GIC, таймер, планировщик целиком | `just smoke` |
| плата | адреса, EL, UART, реальные тайминги | `probe/` ([[h2_minimal_image]]), потом `schedtest` |

Правило дисциплины: **policy-крейт не должен зависеть от `fw-bsp-*`/`fw-arch-*`**. Если понадобилось — граница проведена неверно.

## Baseline до миграции

Без эталона «поведение не изменилось» проверить нечем. Снять до любых правок:

```bash
cargo build && cargo run --features selftest > /tmp/before-selftest.log 2>&1 || true
FW_TIMEOUT=20 just smoke && cp -r target/logs /tmp/before-logs
```

После миграции сравнивать не байты, а маркеры: `Hello from QEMU virt!`, `MMU on!`, `VBAR:`, `sched: … ok`, `selftest: OK`, `ticks = N (+1)`. Адреса (`VBAR`, `stack_top`) могут сдвинуться — это нормально, другой расклад линковки.

## Границы, которые придётся решить

| Вопрос | Рекомендация | Почему |
|---|---|---|
| Куда едет `_start` | остаётся в `firmware` | обнуление `.bss`, стек, понижение EL — это про конкретный образ, а не про переиспользуемый механизм |
| Что тогда в `fw-arch-a64` | векторы + trampoline, `sched_switch`, обёртки sysreg, `IrqSafeSpinLock` | нужны `asm!` и DAIF; host-тесты там невозможны по определению |
| `global_asm!` в библиотеке | допустим | секции `.vectors`/`.text` склеиваются линкером через границы крейтов, `linker.ld` менять не нужно |
| `IrqSafeSpinLock` в `fw-sync` | нет | иначе `fw-sync` перестаёт тестироваться на хосте |
| `selftest`/`schedtest` | только feature `firmware` | features унифицируются по графу зависимостей |
| `#[panic_handler]`, `#[global_allocator]` | только в `firmware` | два таких элемента в графе — ошибка линковки |

## Откат

Миграция — один коммит «chore: workspace skeleton» плюс отдельные коммиты на каждый крейт. Откат любого шага — `git revert` соответствующего коммита; правки asm в миграцию не входят, поэтому конфликтов быть не должно.

## DoD

- `cargo fw` собирается, `cargo fwr` даёт прежний вывод QEMU;
- `just smoke` зелёный во всех четырёх сценариях;
- `cargo fwc` чист;
- в манифестах членов нет `[profile.*]`;
- `cargo build` из корня не собирает прошивку (или падает ожидаемо, пока нет host-крейтов).

## Связанные заметки
- [[0008_workspace_layout]], [[roadmap_5_12]], [[cargo_aliases]], [[no_std_testing]], [[stage5_tdd]], [[h2_minimal_image]]
